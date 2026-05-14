use tonic::{metadata::MetadataMap, service::Interceptor, Request, Status};

#[derive(Clone)]
pub(crate) struct BearerAuth {
    token: String,
}

impl BearerAuth {
    pub(crate) fn new(token: impl Into<String>) -> Self {
        Self { token: token.into() }
    }
}

impl Interceptor for BearerAuth {
    fn call(&mut self, request: Request<()>) -> Result<Request<()>, Status> {
        authorize_metadata(request.metadata(), &self.token)?;
        Ok(request)
    }
}

fn authorize_metadata(metadata: &MetadataMap, expected_token: &str) -> Result<(), Status> {
    let Some(value) = metadata.get("authorization") else {
        return Err(Status::unauthenticated("missing bearer token"));
    };
    let Ok(value) = value.to_str() else {
        return Err(Status::unauthenticated("malformed bearer token"));
    };
    let Some(token) = value.strip_prefix("Bearer ") else {
        return Err(Status::unauthenticated("malformed bearer token"));
    };
    if token != expected_token {
        return Err(Status::unauthenticated("invalid bearer token"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use tonic::metadata::{MetadataMap, MetadataValue};

    use super::*;

    #[test]
    fn grpc_rejects_missing_bearer_token() {
        let error = authorize_metadata(&MetadataMap::new(), "secret").unwrap_err();

        assert_eq!(error.code(), tonic::Code::Unauthenticated);
    }

    #[test]
    fn grpc_rejects_bad_bearer_token() {
        let mut metadata = MetadataMap::new();
        metadata.insert("authorization", MetadataValue::from_static("Bearer wrong"));

        let error = authorize_metadata(&metadata, "secret").unwrap_err();

        assert_eq!(error.code(), tonic::Code::Unauthenticated);
    }

    #[test]
    fn grpc_rejects_malformed_bearer_token() {
        let mut metadata = MetadataMap::new();
        metadata.insert("authorization", MetadataValue::from_static("Basic secret"));

        let error = authorize_metadata(&metadata, "secret").unwrap_err();

        assert_eq!(error.code(), tonic::Code::Unauthenticated);
    }

    #[test]
    fn grpc_accepts_valid_bearer_token() {
        let mut metadata = MetadataMap::new();
        metadata.insert("authorization", MetadataValue::from_static("Bearer secret"));

        authorize_metadata(&metadata, "secret").unwrap();
    }
}
