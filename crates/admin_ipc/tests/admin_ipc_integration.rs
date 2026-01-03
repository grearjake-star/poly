#![cfg(unix)]

use std::sync::{Arc, Mutex};

use admin_ipc::{send_request, AdminRequest, AdminResponse, AdminStatus};
use anyhow::anyhow;
use tokio::time::{sleep, Duration};

#[tokio::test]
async fn status_pause_resume_flow() {
    let dir = tempfile::tempdir().expect("temp dir");
    let socket_path = dir.path().join("admin.sock");
    let socket_str = socket_path
        .to_str()
        .expect("socket path should be utf-8")
        .to_string();

    let risk_state = Arc::new(Mutex::new(String::from("running")));
    let handler_state = Arc::clone(&risk_state);

    let server_task = tokio::spawn(admin_ipc::run_server(&socket_str, move |req| {
        let mut state = handler_state
            .lock()
            .map_err(|_| anyhow!("state poisoned"))?;

        match req {
            AdminRequest::Status => Ok(AdminResponse::Status(AdminStatus {
                run_id: "run-123".to_string(),
                risk_state: state.clone(),
            })),
            AdminRequest::Pause => {
                *state = "paused".to_string();
                Ok(AdminResponse::Ack)
            }
            AdminRequest::Resume => {
                *state = "running".to_string();
                Ok(AdminResponse::Ack)
            }
        }
    }));

    // Allow the server task to start listening.
    sleep(Duration::from_millis(50)).await;

    let initial = send_request(&socket_str, &AdminRequest::Status)
        .await
        .expect("initial status");
    match initial {
        AdminResponse::Status(AdminStatus { risk_state, .. }) => {
            assert_eq!(risk_state, "running");
        }
        _ => panic!("expected status response"),
    }

    let pause_resp = send_request(&socket_str, &AdminRequest::Pause)
        .await
        .expect("pause resp");
    assert!(matches!(pause_resp, AdminResponse::Ack));

    let paused = send_request(&socket_str, &AdminRequest::Status)
        .await
        .expect("paused status");
    match paused {
        AdminResponse::Status(AdminStatus { risk_state, .. }) => {
            assert_eq!(risk_state, "paused");
        }
        _ => panic!("expected status response after pause"),
    }

    let resume_resp = send_request(&socket_str, &AdminRequest::Resume)
        .await
        .expect("resume resp");
    assert!(matches!(resume_resp, AdminResponse::Ack));

    let resumed = send_request(&socket_str, &AdminRequest::Status)
        .await
        .expect("resumed status");
    match resumed {
        AdminResponse::Status(AdminStatus { risk_state, .. }) => {
            assert_eq!(risk_state, "running");
        }
        _ => panic!("expected status response after resume"),
    }

    server_task.abort();

    // Cleanup the socket file explicitly for extra safety.
    let _ = std::fs::remove_file(socket_path);
}
