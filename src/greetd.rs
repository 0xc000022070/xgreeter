use std::path::Path;
use std::time::Duration;

use anyhow::{Context, Result};
use tokio::net::UnixStream;
use tokio::sync::mpsc;

// Re-export the wire types so the rest of the app has a single import path and
// never depends on greetd_ipc's module layout directly.
pub use greetd_ipc::{AuthMessageType, ErrorType, Request, Response};
use greetd_ipc::codec::TokioCodec;

// greetd answers exactly one Response per Request, so real socket and mock can
// share one ordered send/receive pipe.
pub struct Channels {
    pub req_tx: mpsc::Sender<Request>,
    pub resp_rx: mpsc::Receiver<Response>,
}

// Owns the socket on a background task; all IO stays here, the UI only sees channels.
pub async fn spawn_real(sock: &Path) -> Result<Channels> {
    let stream = UnixStream::connect(sock)
        .await
        .with_context(|| format!("connecting to greetd socket {}", sock.display()))?;
    let (mut rd, mut wr) = stream.into_split();

    let (req_tx, mut req_rx) = mpsc::channel::<Request>(8);
    let (resp_tx, resp_rx) = mpsc::channel::<Response>(8);

    tokio::spawn(async move {
        while let Some(req) = req_rx.recv().await {
            if req.write_to(&mut wr).await.is_err() {
                break;
            }
            match Response::read_from(&mut rd).await {
                Ok(resp) => {
                    if resp_tx.send(resp).await.is_err() {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
    });

    Ok(Channels { req_tx, resp_rx })
}

// In-process fake greetd for --demo and tests: one password prompt, `demo` or
// empty succeeds, anything else fails. No PAM, no session launch.
pub fn spawn_mock() -> Channels {
    let (req_tx, mut req_rx) = mpsc::channel::<Request>(8);
    let (resp_tx, resp_rx) = mpsc::channel::<Response>(8);

    tokio::spawn(async move {
        while let Some(req) = req_rx.recv().await {
            let resp = mock_reply(&req);
            // Small latency so the "authenticating" phase is actually visible.
            tokio::time::sleep(Duration::from_millis(180)).await;
            if resp_tx.send(resp).await.is_err() {
                break;
            }
        }
    });

    Channels { req_tx, resp_rx }
}

pub fn mock_reply(req: &Request) -> Response {
    match req {
        Request::CreateSession { .. } => Response::AuthMessage {
            auth_message_type: AuthMessageType::Secret,
            auth_message: "Password: ".into(),
        },
        Request::PostAuthMessageResponse { response } => match response.as_deref() {
            Some("demo") | Some("") | None => Response::Success,
            _ => Response::Error {
                error_type: ErrorType::AuthError,
                description: "authentication failed".into(),
            },
        },
        Request::StartSession { .. } | Request::CancelSession => Response::Success,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_session_prompts_for_password() {
        let r = mock_reply(&Request::CreateSession {
            username: "0xc000022070".into(),
        });
        assert!(matches!(
            r,
            Response::AuthMessage {
                auth_message_type: AuthMessageType::Secret,
                ..
            }
        ));
    }

    #[test]
    fn correct_password_succeeds_wrong_fails() {
        let ok = mock_reply(&Request::PostAuthMessageResponse {
            response: Some("demo".into()),
        });
        assert!(matches!(ok, Response::Success));

        let bad = mock_reply(&Request::PostAuthMessageResponse {
            response: Some("nope".into()),
        });
        assert!(matches!(
            bad,
            Response::Error {
                error_type: ErrorType::AuthError,
                ..
            }
        ));
    }
}
