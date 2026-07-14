//! Shared helpers for openticker-runtime integration tests.
#![allow(dead_code)]

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[derive(Clone)]
pub struct MockAlpacaResponses {
    orders: Arc<Mutex<String>>,
    positions: Arc<Mutex<String>>,
    account: Arc<Mutex<String>>,
}

impl MockAlpacaResponses {
    pub fn new(orders_body: &str, positions_body: &str, account_body: &str) -> Self {
        Self {
            orders: Arc::new(Mutex::new(orders_body.to_owned())),
            positions: Arc::new(Mutex::new(positions_body.to_owned())),
            account: Arc::new(Mutex::new(account_body.to_owned())),
        }
    }

    pub fn set_account_body(&self, account_body: &str) {
        let mut current = self.account.lock().expect("account body lock");
        account_body.clone_into(&mut current);
    }

    pub fn orders_body(&self) -> String {
        self.orders.lock().expect("orders body lock").clone()
    }

    pub fn positions_body(&self) -> String {
        self.positions.lock().expect("positions body lock").clone()
    }

    pub fn account_body(&self) -> String {
        self.account.lock().expect("account body lock").clone()
    }
}

pub struct MockAlpacaServer {
    pub base_url: String,
    stop: Arc<AtomicBool>,
    handle: thread::JoinHandle<Vec<String>>,
}

impl MockAlpacaServer {
    pub fn spawn(responses: MockAlpacaResponses) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("mock server should bind");
        listener
            .set_nonblocking(true)
            .expect("mock server should be non-blocking");
        let address = listener
            .local_addr()
            .expect("mock server local addr should resolve");
        let stop = Arc::new(AtomicBool::new(false));
        let stop_signal = Arc::clone(&stop);

        let handle = thread::spawn(move || {
            let mut paths = Vec::new();
            while !stop_signal.load(Ordering::SeqCst) {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        let request = read_http_request(&mut stream);
                        if request.is_empty() {
                            continue;
                        }
                        let path = request_path(&request);
                        paths.push(path.clone());

                        let (status, body) = if path.starts_with("/v2/orders") {
                            ("HTTP/1.1 200 OK", responses.orders_body())
                        } else if path == "/v2/positions" {
                            ("HTTP/1.1 200 OK", responses.positions_body())
                        } else if path == "/v2/account" {
                            ("HTTP/1.1 200 OK", responses.account_body())
                        } else {
                            (
                                "HTTP/1.1 404 Not Found",
                                "{\"error\":\"not found\"}".to_owned(),
                            )
                        };
                        write_http_response(&mut stream, status, body.as_str());
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(10));
                    }
                    Err(_) => break,
                }
            }
            paths
        });

        Self {
            base_url: format!("http://{address}"),
            stop,
            handle,
        }
    }

    pub fn shutdown(self) -> Vec<String> {
        self.stop.store(true, Ordering::SeqCst);
        self.handle
            .join()
            .expect("mock server should shut down cleanly")
    }
}

pub fn read_http_request(stream: &mut TcpStream) -> String {
    let mut buffer = [0_u8; 1024];
    let mut data = Vec::new();
    for _ in 0..200 {
        match stream.read(&mut buffer) {
            Ok(0) => break,
            Ok(count) => {
                data.extend_from_slice(&buffer[..count]);
                if data.windows(4).any(|window| window == b"\r\n\r\n") {
                    break;
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(2));
            }
            Err(_) => break,
        }
    }
    String::from_utf8_lossy(&data).into_owned()
}

pub fn request_path(request: &str) -> String {
    request
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .unwrap_or("/")
        .to_owned()
}

pub fn write_http_response(stream: &mut TcpStream, status: &str, body: &str) {
    let response = format!(
        "{status}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
        body.len()
    );
    stream
        .write_all(response.as_bytes())
        .expect("response write should succeed");
}

pub fn create_temp_db_path(prefix: &str) -> PathBuf {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock should be monotonic")
        .as_nanos();
    std::env::temp_dir().join(format!("openticker-runtime-{prefix}-{timestamp}.db"))
}
