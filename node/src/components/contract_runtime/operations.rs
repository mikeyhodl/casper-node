use std::{cmp, collections::BTreeMap, ops::Range, sync::Arc, time::Instant};

use itertools::Itertools;
use tracing::{debug, error, info, trace, warn};

use casper_execution_engine::{
    core::engine_state::{
        self,
        purge::{PurgeConfig, PurgeResult},
        step::EvictItem,
        DeployItem, EngineState, ExecuteRequest, ExecutionResult as EngineExecutionResult,
        GetEraValidatorsRequest, RewardItem, StepError, StepRequest, StepSuccess,
    },
    shared::{additive_map::AdditiveMap, newtypes::CorrelationId, transform::Transform},
    storage::global_state::lmdb::LmdbGlobalState,
};
use casper_hashing::Digest;
use casper_types::{DeployHash, EraId, ExecutionResult, Key, ProtocolVersion, PublicKey, U512};

use crate::{
    components::{
        consensus::EraReport,
        contract_runtime::{
            error::BlockExecutionError, types::StepEffectAndUpcomingEraValidators,
            BlockAndExecutionEffects, ExecutionPreState, Metrics,
        },
    },
    types::{ActivationPoint, Block, Deploy, DeployHeader, FinalizedBlock},
};
use casper_execution_engine::{
    core::{engine_state::execution_result::ExecutionResults, execution},
    storage::global_state::{CommitProvider, StateProvider},
};

fn generate_range_by_index(n: u64, m: u64, index: u64) -> Option<Range<u64>> {
    let start = index.checked_mul(m)?;
    let end = cmp::min(start.checked_add(m)?, n);
    Some(start..end)
}

/// Calculates era keys to be pruned.
///
/// Outcomes:
/// * Ok(Some(range)) -- these keys should be purged
/// * Ok(None) -- nothing to do, either done, or there is not enough eras to purge
fn calculate_purge_eras(
    activation_era_id: EraId,
    activation_height: u64,
    current_height: u64,
    batch_size: u64,
) -> Option<Vec<Key>> {
    if batch_size == 0 {
        // Nothing to do, the batch size is 0.
        return None;
    }

    let nth_chunk: u64 = match current_height.checked_sub(activation_height) {
        Some(nth_chunk) => nth_chunk,
        None => {
            // Time went backwards, programmer error, etc
            return None;
        }
    };

    let range = generate_range_by_index(activation_era_id.value(), batch_size, nth_chunk)?;

    if range.is_empty() {
        return None;
    }

    Some(range.map(EraId::new).map(Key::EraInfo).collect())
}

/// Executes a finalized block.
#[allow(clippy::too_many_arguments)]
pub fn execute_finalized_block(
    engine_state: &EngineState<LmdbGlobalState>,
    metrics: Option<Arc<Metrics>>,
    protocol_version: ProtocolVersion,
    execution_pre_state: ExecutionPreState,
    finalized_block: FinalizedBlock,
    deploys: Vec<Deploy>,
    transfers: Vec<Deploy>,
    activation_point: ActivationPoint,
    activation_point_block_height: Option<u64>,
    purge_batch_size: u64,
) -> Result<BlockAndExecutionEffects, BlockExecutionError> {
    if finalized_block.height() != execution_pre_state.next_block_height {
        return Err(BlockExecutionError::WrongBlockHeight {
            finalized_block: Box::new(finalized_block),
            execution_pre_state: Box::new(execution_pre_state),
        });
    }
    let ExecutionPreState {
        pre_state_root_hash,
        parent_hash,
        parent_seed,
        next_block_height: _,
    } = execution_pre_state;
    let mut state_root_hash = pre_state_root_hash;
    let mut execution_results: Vec<(_, DeployHeader, ExecutionResult)> =
        Vec::with_capacity(deploys.len() + transfers.len());
    // Run any deploys that must be executed
    let block_time = finalized_block.timestamp().millis();
    let start = Instant::now();

    // Create a new EngineState that reads from LMDB but only caches changes in memory.
    let scratch_state = engine_state.get_scratch_engine_state();

    for deploy in deploys.into_iter().chain(transfers) {
        let deploy_hash = *deploy.id();
        let deploy_header = deploy.header().clone();
        let execute_request = ExecuteRequest::new(
            state_root_hash,
            block_time,
            vec![DeployItem::from(deploy)],
            protocol_version,
            finalized_block.proposer().clone(),
        );

        // TODO: this is currently working coincidentally because we are passing only one
        // deploy_item per exec. The execution results coming back from the ee lacks the
        // mapping between deploy_hash and execution result, and this outer logic is
        // enriching it with the deploy hash. If we were passing multiple deploys per exec
        // the relation between the deploy and the execution results would be lost.
        let result = execute(&scratch_state, metrics.clone(), execute_request)?;

        trace!(?deploy_hash, ?result, "deploy execution result");
        // As for now a given state is expected to exist.
        let (state_hash, execution_result) = commit_execution_effects(
            &scratch_state,
            metrics.clone(),
            state_root_hash,
            deploy_hash.into(),
            result,
        )?;
        execution_results.push((deploy_hash, deploy_header, execution_result));
        state_root_hash = state_hash;
    }

    if let Some(metrics) = metrics.as_ref() {
        metrics.exec_block.observe(start.elapsed().as_secs_f64());
    }

    // If the finalized block has an era report, run the auction contract and get the upcoming era
    // validators
    let maybe_step_effect_and_upcoming_era_validators =
        if let Some(era_report) = finalized_block.era_report() {
            let StepSuccess {
                post_state_hash: _, // ignore the post-state-hash returned from scratch
                execution_journal: step_execution_journal,
            } = commit_step(
                &scratch_state, // engine_state
                metrics.clone(),
                protocol_version,
                state_root_hash,
                era_report,
                finalized_block.timestamp().millis(),
                finalized_block.era_id().successor(),
            )?;

            state_root_hash =
                engine_state.write_scratch_to_db(state_root_hash, scratch_state.into_inner())?;

            // In this flow we execute using a recent state root hash where the system contract
            // registry is guaranteed to exist.
            let system_contract_registry = None;

            let upcoming_era_validators = engine_state.get_era_validators(
                CorrelationId::new(),
                system_contract_registry,
                GetEraValidatorsRequest::new(state_root_hash, protocol_version),
            )?;
            Some(StepEffectAndUpcomingEraValidators {
                step_execution_journal,
                upcoming_era_validators,
            })
        } else {
            // Finally, the new state-root-hash from the cumulative changes to global state is
            // returned when they are written to LMDB.
            state_root_hash =
                engine_state.write_scratch_to_db(state_root_hash, scratch_state.into_inner())?;
            None
        };

    // Flush once, after all deploys have been executed.
    engine_state.flush_environment()?;

    // Update the metric.
    let block_height = finalized_block.height();
    if let Some(metrics) = metrics.as_ref() {
        metrics.chain_height.set(block_height as i64);
    }

    let next_era_validator_weights: Option<BTreeMap<PublicKey, U512>> =
        maybe_step_effect_and_upcoming_era_validators
            .as_ref()
            .and_then(
                |StepEffectAndUpcomingEraValidators {
                     upcoming_era_validators,
                     ..
                 }| {
                    upcoming_era_validators
                        .get(&finalized_block.era_id().successor())
                        .cloned()
                },
            );

    // Purging

    match (activation_point, activation_point_block_height) {
        (ActivationPoint::EraId(activation_point), Some(activation_point_block_height)) => {
            let previous_block_height = finalized_block.height() - 1;

            match calculate_purge_eras(
                activation_point,
                activation_point_block_height,
                previous_block_height,
                purge_batch_size,
            ) {
                Some(keys_to_purge) => {
                    let first_key = keys_to_purge.first().copied();
                    let last_key = keys_to_purge.last().copied();
                    info!(previous_block_height, %activation_point_block_height, %state_root_hash, first_key=?first_key, last_key=?last_key, "commit_purge: preparing purge config");
                    let purge_config = PurgeConfig::new(state_root_hash, keys_to_purge);
                    match engine_state.commit_purge(CorrelationId::new(), purge_config) {
                        Ok(PurgeResult::RootNotFound) => {
                            error!(previous_block_height, %state_root_hash, "commit_purge: root not found");
                        }
                        Ok(PurgeResult::DoesNotExist) => {
                            warn!(previous_block_height, %state_root_hash, "commit_purge: key does not exist");
                        }
                        Ok(PurgeResult::Success { post_state_hash }) => {
                            info!(previous_block_height, %activation_point_block_height, %state_root_hash, %post_state_hash, first_key=?first_key, last_key=?last_key, "commit_purge: success");
                            state_root_hash = post_state_hash;
                        }
                        Err(error) => {
                            error!(previous_block_height, %activation_point_block_height, %error, "commit_purge: commit purge error");
                            return Err(error.into());
                        }
                    }
                }
                None => {
                    info!(previous_block_height, %activation_point_block_height, "commit_purge: nothing to do, no more eras to delete");
                }
            }
        }
        (ActivationPoint::EraId(era_id), None) => {
            info!(activation_point=?era_id, activation_point_block_height, "commit_purge: nothing to do; no activation block height found");
        }
        (ActivationPoint::Genesis(_timestamp), Some(activation_height)) => {
            warn!(
                activation_height,
                "found activation block height but activation point is a genesis"
            );
        }
        (ActivationPoint::Genesis(_timestamp), None) => {
            warn!(
                ?activation_point,
                "activation point is genesis and no activation height found"
            );
        }
    }

    let block = Block::new(
        parent_hash,
        parent_seed,
        state_root_hash,
        finalized_block,
        next_era_validator_weights,
        protocol_version,
    )?;

    let patched_execution_results = execution_results
        .into_iter()
        .map(|(hash, header, result)| (hash, (header, result)))
        .collect();

    Ok(BlockAndExecutionEffects {
        block,
        execution_results: patched_execution_results,
        maybe_step_effect_and_upcoming_era_validators,
    })
}

/// Commits the execution effects.
fn commit_execution_effects<S>(
    engine_state: &EngineState<S>,
    metrics: Option<Arc<Metrics>>,
    state_root_hash: Digest,
    deploy_hash: DeployHash,
    execution_results: ExecutionResults,
) -> Result<(Digest, ExecutionResult), BlockExecutionError>
where
    S: StateProvider + CommitProvider,
    S::Error: Into<execution::Error>,
{
    let ee_execution_result = execution_results
        .into_iter()
        .exactly_one()
        .map_err(|_| BlockExecutionError::MoreThanOneExecutionResult)?;
    let json_execution_result = ExecutionResult::from(&ee_execution_result);

    let execution_effect: AdditiveMap<Key, Transform> = match ee_execution_result {
        EngineExecutionResult::Success {
            execution_journal,
            cost,
            ..
        } => {
            // We do want to see the deploy hash and cost in the logs.
            // We don't need to see the effects in the logs.
            debug!(?deploy_hash, %cost, "execution succeeded");
            execution_journal
        }
        EngineExecutionResult::Failure {
            error,
            execution_journal,
            cost,
            ..
        } => {
            // Failure to execute a contract is a user error, not a system error.
            // We do want to see the deploy hash, error, and cost in the logs.
            // We don't need to see the effects in the logs.
            debug!(?deploy_hash, ?error, %cost, "execution failure");
            execution_journal
        }
    }
    .into();
    let new_state_root =
        commit_transforms(engine_state, metrics, state_root_hash, execution_effect)?;
    Ok((new_state_root, json_execution_result))
}

fn commit_transforms<S>(
    engine_state: &EngineState<S>,
    metrics: Option<Arc<Metrics>>,
    state_root_hash: Digest,
    effects: AdditiveMap<Key, Transform>,
) -> Result<Digest, engine_state::Error>
where
    S: StateProvider + CommitProvider,
    S::Error: Into<execution::Error>,
{
    trace!(?state_root_hash, ?effects, "commit");
    let correlation_id = CorrelationId::new();
    let start = Instant::now();
    let result = engine_state.apply_effect(correlation_id, state_root_hash, effects);
    if let Some(metrics) = metrics {
        metrics.apply_effect.observe(start.elapsed().as_secs_f64());
    }
    trace!(?result, "commit result");
    result.map(Digest::from)
}

fn execute<S>(
    engine_state: &EngineState<S>,
    metrics: Option<Arc<Metrics>>,
    execute_request: ExecuteRequest,
) -> Result<ExecutionResults, engine_state::Error>
where
    S: StateProvider + CommitProvider,
    S::Error: Into<execution::Error>,
{
    trace!(?execute_request, "execute");
    let correlation_id = CorrelationId::new();
    let start = Instant::now();
    let result = engine_state.run_execute(correlation_id, execute_request);
    if let Some(metrics) = metrics {
        metrics.run_execute.observe(start.elapsed().as_secs_f64());
    }
    trace!(?result, "execute result");
    result
}

fn commit_step<S>(
    engine_state: &EngineState<S>,
    maybe_metrics: Option<Arc<Metrics>>,
    protocol_version: ProtocolVersion,
    pre_state_root_hash: Digest,
    era_report: &EraReport<PublicKey>,
    era_end_timestamp_millis: u64,
    next_era_id: EraId,
) -> Result<StepSuccess, StepError>
where
    S: StateProvider + CommitProvider,
    S::Error: Into<execution::Error>,
{
    // Extract the rewards and the inactive validators if this is a switch block
    let EraReport {
        equivocators,
        rewards,
        inactive_validators,
    } = era_report;

    let reward_items = rewards
        .iter()
        .map(|(vid, value)| RewardItem::new(vid.clone(), *value))
        .collect();

    // Both inactive validators and equivocators are evicted
    let evict_items = inactive_validators
        .iter()
        .chain(equivocators)
        .cloned()
        .map(EvictItem::new)
        .collect();

    let step_request = StepRequest {
        pre_state_hash: pre_state_root_hash,
        protocol_version,
        reward_items,
        // Note: The Casper Network does not slash, but another network could
        slash_items: vec![],
        evict_items,
        next_era_id,
        era_end_timestamp_millis,
    };

    // Have the EE commit the step.
    let correlation_id = CorrelationId::new();
    let start = Instant::now();
    let result = engine_state.commit_step(correlation_id, step_request);
    if let Some(metrics) = maybe_metrics {
        let elapsed = start.elapsed().as_secs_f64();
        metrics.commit_step.observe(elapsed);
        metrics.latest_commit_step.set(elapsed);
    }
    trace!(?result, "step response");
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn calculation_is_safe_with_invalid_input() {
        assert_eq!(calculate_purge_eras(EraId::new(0), 0, 0, 0,), None);
        assert_eq!(calculate_purge_eras(EraId::new(u64::MAX), 0, 0, 0,), None);
        assert_eq!(
            calculate_purge_eras(EraId::new(u64::MAX), 1, u64::MAX, u64::MAX),
            None
        );
    }
    #[test]
    fn calculation_is_lazy() {
        // NOTE: Range of EraInfos is lazy, so it does not consume memory, but getting the last
        // batch out of u64::MAX of erainfos needs to iterate over all chunks.
        assert!(calculate_purge_eras(EraId::new(u64::MAX), 0, u64::MAX, 100,).is_none(),);
        assert_eq!(
            calculate_purge_eras(EraId::new(u64::MAX), 1, 100, 100,)
                .unwrap()
                .len(),
            100
        );
    }
    #[test]
    fn should_calculate_prune_eras() {
        let activation_height = 50;
        let current_height = 50;
        const ACTIVATION_POINT_ERA_ID: EraId = EraId::new(5);

        // batch size 1

        assert_eq!(
            calculate_purge_eras(
                ACTIVATION_POINT_ERA_ID,
                activation_height,
                current_height,
                1
            ),
            Some(vec![Key::EraInfo(EraId::new(0))])
        );
        assert_eq!(
            calculate_purge_eras(
                ACTIVATION_POINT_ERA_ID,
                activation_height,
                current_height + 1,
                1
            ),
            Some(vec![Key::EraInfo(EraId::new(1))])
        );
        assert_eq!(
            calculate_purge_eras(
                ACTIVATION_POINT_ERA_ID,
                activation_height,
                current_height + 2,
                1
            ),
            Some(vec![Key::EraInfo(EraId::new(2))])
        );
        assert_eq!(
            calculate_purge_eras(
                ACTIVATION_POINT_ERA_ID,
                activation_height,
                current_height + 3,
                1
            ),
            Some(vec![Key::EraInfo(EraId::new(3))])
        );
        assert_eq!(
            calculate_purge_eras(
                ACTIVATION_POINT_ERA_ID,
                activation_height,
                current_height + 4,
                1
            ),
            Some(vec![Key::EraInfo(EraId::new(4))])
        );
        assert_eq!(
            calculate_purge_eras(
                ACTIVATION_POINT_ERA_ID,
                activation_height,
                current_height + 5,
                1
            ),
            None,
        );
        assert_eq!(
            calculate_purge_eras(ACTIVATION_POINT_ERA_ID, activation_height, u64::MAX, 1),
            None,
        );

        // batch size 2

        assert_eq!(
            calculate_purge_eras(
                ACTIVATION_POINT_ERA_ID,
                activation_height,
                current_height,
                2
            ),
            Some(vec![
                Key::EraInfo(EraId::new(0)),
                Key::EraInfo(EraId::new(1))
            ])
        );
        assert_eq!(
            calculate_purge_eras(
                ACTIVATION_POINT_ERA_ID,
                activation_height,
                current_height + 1,
                2
            ),
            Some(vec![
                Key::EraInfo(EraId::new(2)),
                Key::EraInfo(EraId::new(3))
            ])
        );
        assert_eq!(
            calculate_purge_eras(
                ACTIVATION_POINT_ERA_ID,
                activation_height,
                current_height + 2,
                2
            ),
            Some(vec![Key::EraInfo(EraId::new(4))])
        );
        assert_eq!(
            calculate_purge_eras(
                ACTIVATION_POINT_ERA_ID,
                activation_height,
                current_height + 3,
                2
            ),
            None
        );
        assert_eq!(
            calculate_purge_eras(ACTIVATION_POINT_ERA_ID, activation_height, u64::MAX, 2),
            None,
        );

        // batch size 3

        assert_eq!(
            calculate_purge_eras(
                ACTIVATION_POINT_ERA_ID,
                activation_height,
                current_height,
                3
            ),
            Some(vec![
                Key::EraInfo(EraId::new(0)),
                Key::EraInfo(EraId::new(1)),
                Key::EraInfo(EraId::new(2)),
            ])
        );
        assert_eq!(
            calculate_purge_eras(
                ACTIVATION_POINT_ERA_ID,
                activation_height,
                current_height + 1,
                3
            ),
            Some(vec![
                Key::EraInfo(EraId::new(3)),
                Key::EraInfo(EraId::new(4)),
            ])
        );

        assert_eq!(
            calculate_purge_eras(
                ACTIVATION_POINT_ERA_ID,
                activation_height,
                current_height + 2,
                3
            ),
            None
        );
        assert_eq!(
            calculate_purge_eras(ACTIVATION_POINT_ERA_ID, activation_height, u64::MAX, 3),
            None,
        );

        // batch size 4

        assert_eq!(
            calculate_purge_eras(
                ACTIVATION_POINT_ERA_ID,
                activation_height,
                current_height,
                4
            ),
            Some(vec![
                Key::EraInfo(EraId::new(0)),
                Key::EraInfo(EraId::new(1)),
                Key::EraInfo(EraId::new(2)),
                Key::EraInfo(EraId::new(3)),
            ])
        );
        assert_eq!(
            calculate_purge_eras(
                ACTIVATION_POINT_ERA_ID,
                activation_height,
                current_height + 1,
                4
            ),
            Some(vec![Key::EraInfo(EraId::new(4)),])
        );

        assert_eq!(
            calculate_purge_eras(
                ACTIVATION_POINT_ERA_ID,
                activation_height,
                current_height + 2,
                4
            ),
            None
        );
        assert_eq!(
            calculate_purge_eras(ACTIVATION_POINT_ERA_ID, activation_height, u64::MAX, 4),
            None,
        );

        // batch size 5

        assert_eq!(
            calculate_purge_eras(
                ACTIVATION_POINT_ERA_ID,
                activation_height,
                current_height,
                5
            ),
            Some(vec![
                Key::EraInfo(EraId::new(0)),
                Key::EraInfo(EraId::new(1)),
                Key::EraInfo(EraId::new(2)),
                Key::EraInfo(EraId::new(3)),
                Key::EraInfo(EraId::new(4)),
            ])
        );
        assert_eq!(
            calculate_purge_eras(
                ACTIVATION_POINT_ERA_ID,
                activation_height,
                current_height + 1,
                5
            ),
            None,
        );

        assert_eq!(
            calculate_purge_eras(
                ACTIVATION_POINT_ERA_ID,
                activation_height,
                current_height + 2,
                5
            ),
            None
        );
        assert_eq!(
            calculate_purge_eras(ACTIVATION_POINT_ERA_ID, activation_height, u64::MAX, 5),
            None,
        );

        // batch size 6

        assert_eq!(
            calculate_purge_eras(
                ACTIVATION_POINT_ERA_ID,
                activation_height,
                current_height,
                6
            ),
            Some(vec![
                Key::EraInfo(EraId::new(0)),
                Key::EraInfo(EraId::new(1)),
                Key::EraInfo(EraId::new(2)),
                Key::EraInfo(EraId::new(3)),
                Key::EraInfo(EraId::new(4)),
            ])
        );
        assert_eq!(
            calculate_purge_eras(
                ACTIVATION_POINT_ERA_ID,
                activation_height,
                current_height + 1,
                6
            ),
            None,
        );

        assert_eq!(
            calculate_purge_eras(
                ACTIVATION_POINT_ERA_ID,
                activation_height,
                current_height + 2,
                6
            ),
            None
        );
        assert_eq!(
            calculate_purge_eras(ACTIVATION_POINT_ERA_ID, activation_height, u64::MAX, 6),
            None,
        );

        // batch size max

        assert_eq!(
            calculate_purge_eras(
                ACTIVATION_POINT_ERA_ID,
                activation_height,
                current_height,
                u64::MAX,
            ),
            Some(vec![
                Key::EraInfo(EraId::new(0)),
                Key::EraInfo(EraId::new(1)),
                Key::EraInfo(EraId::new(2)),
                Key::EraInfo(EraId::new(3)),
                Key::EraInfo(EraId::new(4)),
            ])
        );
        assert_eq!(
            calculate_purge_eras(
                ACTIVATION_POINT_ERA_ID,
                activation_height,
                current_height + 1,
                u64::MAX,
            ),
            None,
        );

        assert_eq!(
            calculate_purge_eras(
                ACTIVATION_POINT_ERA_ID,
                activation_height,
                current_height + 2,
                u64::MAX,
            ),
            None
        );
        assert_eq!(
            calculate_purge_eras(
                ACTIVATION_POINT_ERA_ID,
                activation_height,
                u64::MAX,
                u64::MAX,
            ),
            None,
        );
    }
}
