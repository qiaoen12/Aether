use async_stream::stream;
use axum::body::Body;
use axum::http::Response;
use futures_util::StreamExt;

use crate::concurrency::ConcurrencyPermit;

pub struct AdmissionPermit {
    _local: Vec<ConcurrencyPermit>,
    _distributed: Option<Box<dyn Send + Sync>>,
}

impl std::fmt::Debug for AdmissionPermit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AdmissionPermit")
            .field("local_count", &self._local.len())
            .field("has_distributed", &self._distributed.is_some())
            .finish()
    }
}

impl AdmissionPermit {
    pub fn from_parts<D: Send + Sync + 'static>(
        local: Option<ConcurrencyPermit>,
        distributed: Option<D>,
    ) -> Option<Self> {
        if local.is_none() && distributed.is_none() {
            None
        } else {
            Some(Self {
                _local: local.into_iter().collect(),
                _distributed: distributed.map(|permit| Box::new(permit) as Box<dyn Send + Sync>),
            })
        }
    }

    pub fn with_local(mut self, permit: ConcurrencyPermit) -> Self {
        self._local.push(permit);
        self
    }
}

impl From<ConcurrencyPermit> for AdmissionPermit {
    fn from(value: ConcurrencyPermit) -> Self {
        Self {
            _local: vec![value],
            _distributed: None,
        }
    }
}

pub fn maybe_hold_axum_response_permit(
    response: Response<Body>,
    permit: Option<AdmissionPermit>,
) -> Response<Body> {
    match permit {
        Some(permit) => hold_axum_response_permit(response, permit),
        None => response,
    }
}

pub async fn hold_admission_permit_until<T, F>(permit: Option<AdmissionPermit>, future: F) -> T
where
    F: std::future::Future<Output = T>,
{
    let _permit = permit;
    future.await
}

fn hold_axum_response_permit(response: Response<Body>, permit: AdmissionPermit) -> Response<Body> {
    let (parts, body) = response.into_parts();
    let stream = stream! {
        let _permit = permit;
        let mut body_stream = body.into_data_stream();
        while let Some(item) = body_stream.next().await {
            yield item;
        }
    };
    Response::from_parts(parts, Body::from_stream(stream))
}

#[cfg(test)]
mod tests {
    use super::{hold_admission_permit_until, maybe_hold_axum_response_permit, AdmissionPermit};
    use crate::ConcurrencyGate;
    use axum::body::{to_bytes, Body};
    use axum::http::Response;

    #[tokio::test]
    async fn holds_permit_until_response_body_is_consumed() {
        let gate = ConcurrencyGate::new("test", 1);
        let permit = gate.try_acquire().expect("first permit");
        let response = Response::new(Body::from_stream(
            async_stream::stream! { yield Ok::<_, std::convert::Infallible>(axum::body::Bytes::from_static(b"ok")); },
        ));

        let wrapped = maybe_hold_axum_response_permit(response, Some(permit.into()));
        assert_eq!(gate.snapshot().in_flight, 1);
        assert!(gate.try_acquire().is_err(), "permit should still be held");

        let body = to_bytes(wrapped.into_body(), usize::MAX)
            .await
            .expect("body should drain");
        assert_eq!(body.as_ref(), b"ok");
        assert_eq!(gate.snapshot().in_flight, 0);
    }

    #[tokio::test]
    async fn holds_combined_local_and_distributed_permit_until_future_finishes() {
        let local_gate = ConcurrencyGate::new("local", 1);
        let local = local_gate.try_acquire().expect("local permit");
        let distributed_gate = ConcurrencyGate::new("distributed", 1);
        let distributed = distributed_gate.try_acquire().expect("distributed permit");

        let task = tokio::spawn(hold_admission_permit_until(
            AdmissionPermit::from_parts(Some(local), Some(distributed)),
            async {
                tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            },
        ));

        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        assert!(
            local_gate.try_acquire().is_err(),
            "local permit should still be held"
        );
        assert!(
            distributed_gate.try_acquire().is_err(),
            "distributed permit should still be held"
        );

        task.await.expect("task should complete");
        assert_eq!(local_gate.snapshot().in_flight, 0);
        assert_eq!(distributed_gate.snapshot().in_flight, 0);
    }
}
