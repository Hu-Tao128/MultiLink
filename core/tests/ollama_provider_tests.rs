use std::net::TcpListener;
use std::thread;

use multilink_core::providers::ollama::OllamaProvider;
use multilink_core::{LLMProvider, PromptOptions};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener as TokioTcpListener;

#[test]
fn ollama_is_available_checks_tcp_connectivity() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind listener");
    let port = listener.local_addr().expect("read local addr").port();

    let handle = thread::spawn(move || {
        let _ = listener.accept();
    });

    let provider = OllamaProvider::new(format!("http://127.0.0.1:{port}"), "llama3.2".to_string());
    assert!(provider.is_available());

    let _ = handle.join();
}

#[test]
fn ollama_is_available_rejects_invalid_scheme() {
    let provider = OllamaProvider::new("file:///tmp/ollama".to_string(), "llama3.2".to_string());
    assert!(!provider.is_available());
}

#[tokio::test]
async fn ollama_send_retries_transient_server_error() {
    let listener = TokioTcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind test listener");
    let port = listener.local_addr().expect("read local addr").port();

    let server = tokio::spawn(async move {
        for attempt in 0..2 {
            let (mut socket, _) = listener.accept().await.expect("accept");
            let mut buf = vec![0u8; 8192];
            let _ = socket.read(&mut buf).await;

            if attempt == 0 {
                socket
                    .write_all(
                        b"HTTP/1.1 500 Internal Server Error\r\nContent-Length: 5\r\n\r\nerror",
                    )
                    .await
                    .expect("write 500 response");
            } else {
                let body = r#"{"message":{"role":"assistant","content":"ok"}}"#;
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                    body.len(),
                    body
                );
                socket
                    .write_all(response.as_bytes())
                    .await
                    .expect("write 200 response");
            }
        }
    });

    let provider = OllamaProvider::new(format!("http://127.0.0.1:{port}"), "llama3.2".to_string());

    let response = provider
        .send("hello".to_string(), PromptOptions::default())
        .await
        .expect("send should succeed after retry");

    assert_eq!(response.text, "ok");

    server.await.expect("server task should complete");
}
