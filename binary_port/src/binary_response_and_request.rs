use casper_types::{
    bytesrepr::{self, FromBytes, ToBytes},
    ProtocolVersion,
};

use crate::{
    binary_response::BinaryResponse, original_request_context::OriginalRequestContext,
    response_type::PayloadEntity, ResponseType,
};

use crate::record_id::RecordId;
#[cfg(test)]
use casper_types::testing::TestRng;

/// The binary response along with the original binary request attached.
#[derive(Debug, PartialEq)]
pub struct BinaryResponseAndRequest {
    /// Context of the original request.
    original_request: OriginalRequestContext,
    /// The response.
    response: BinaryResponse,
}

impl BinaryResponseAndRequest {
    /// Creates new binary response with the original request attached.
    pub fn new(
        data: BinaryResponse,
        original_request_payload: &[u8],
        original_request_id: u16,
    ) -> Self {
        Self {
            original_request: OriginalRequestContext::new(
                original_request_id,
                original_request_payload.to_vec(),
            ),
            response: data,
        }
    }

    /// Returns a new binary response with specified data and no original request.
    pub fn new_test_response<A: PayloadEntity + ToBytes>(
        record_id: RecordId,
        data: &A,
        protocol_version: ProtocolVersion,
    ) -> BinaryResponseAndRequest {
        let response = BinaryResponse::from_raw_bytes(
            ResponseType::from_record_id(record_id, false),
            data.to_bytes().unwrap(),
            protocol_version,
        );
        Self::new(response, &[], 0)
    }

    /// Returns a new binary response with specified legacy data and no original request.
    pub fn new_legacy_test_response<A: PayloadEntity + serde::Serialize>(
        record_id: RecordId,
        data: &A,
        protocol_version: ProtocolVersion,
    ) -> BinaryResponseAndRequest {
        let response = BinaryResponse::from_raw_bytes(
            ResponseType::from_record_id(record_id, true),
            bincode::serialize(data).unwrap(),
            protocol_version,
        );
        Self::new(response, &[], 0)
    }

    /// Returns true if response is success.
    pub fn is_success(&self) -> bool {
        self.response.is_success()
    }

    /// Returns the error code.
    pub fn error_code(&self) -> u16 {
        self.response.error_code()
    }

    #[cfg(test)]
    pub(crate) fn random(rng: &mut TestRng) -> Self {
        Self {
            original_request: OriginalRequestContext::random(rng),
            response: BinaryResponse::random(rng),
        }
    }

    /// Returns serialized bytes representing the original request.
    pub fn original_request_bytes(&self) -> &[u8] {
        self.original_request.data()
    }

    /// Returns the original request id.
    pub fn original_request_id(&self) -> u16 {
        self.original_request.id()
    }

    /// Returns the inner binary response.
    pub fn response(&self) -> &BinaryResponse {
        &self.response
    }
}

impl ToBytes for BinaryResponseAndRequest {
    fn to_bytes(&self) -> Result<Vec<u8>, bytesrepr::Error> {
        let mut buffer = bytesrepr::allocate_buffer(self)?;
        self.write_bytes(&mut buffer)?;
        Ok(buffer)
    }

    fn write_bytes(&self, writer: &mut Vec<u8>) -> Result<(), bytesrepr::Error> {
        let BinaryResponseAndRequest {
            original_request,
            response,
        } = self;

        original_request.write_bytes(writer)?;
        response.write_bytes(writer)
    }

    fn serialized_length(&self) -> usize {
        self.original_request.serialized_length() + self.response.serialized_length()
    }
}

impl FromBytes for BinaryResponseAndRequest {
    fn from_bytes(bytes: &[u8]) -> Result<(Self, &[u8]), bytesrepr::Error> {
        let (original_request, remainder) = OriginalRequestContext::from_bytes(bytes)?;
        let (response, remainder) = FromBytes::from_bytes(remainder)?;

        Ok((
            BinaryResponseAndRequest {
                original_request,
                response,
            },
            remainder,
        ))
    }
}

impl From<BinaryResponseAndRequest> for BinaryResponse {
    fn from(response_and_request: BinaryResponseAndRequest) -> Self {
        let BinaryResponseAndRequest { response, .. } = response_and_request;
        response
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use casper_types::testing::TestRng;

    #[test]
    fn bytesrepr_roundtrip() {
        let rng = &mut TestRng::new();

        let val = BinaryResponseAndRequest::random(rng);
        bytesrepr::test_serialization_roundtrip(&val);
    }
}