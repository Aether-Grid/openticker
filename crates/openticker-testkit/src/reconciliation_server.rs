use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::thread;
use std::time::{Duration, Instant};

/// Spawns a deterministic fake reconciliation server for Alpaca-style snapshots.
///
/// The server responds to:
/// - `GET /v2/orders`
/// - `GET /v2/positions`
/// - `GET /v2/account`
///
/// It accepts up to `max_requests` requests before exiting and returns observed request lines.
///
/// # Panics
///
/// Panics when binding or configuring the local socket fails, or when the
/// local address cannot be resolved.
#[must_use]
pub fn spawn_fake_reconciliation_server(
    orders_body: &str,
    positions_body: &str,
    account_body: &str,
    max_requests: usize,
) -> (String, thread::JoinHandle<Vec<String>>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("fake server should bind");
    listener
        .set_nonblocking(true)
        .expect("fake server should be non-blocking");
    let addr = listener
        .local_addr()
        .expect("fake server local addr should resolve");

    let orders = orders_body.to_owned();
    let positions = positions_body.to_owned();
    let account = account_body.to_owned();

    let handle = thread::spawn(move || {
        let mut requests = Vec::new();
        let mut idle_deadline = Instant::now() + Duration::from_secs(3);

        while requests.len() < max_requests && Instant::now() < idle_deadline {
            match listener.accept() {
                Ok((mut stream, _)) => {
                    if let Some(request_line) = read_request_line(&mut stream) {
                        let (status, body) = if request_line.starts_with("GET /v2/orders ") {
                            ("200 OK", orders.as_str())
                        } else if request_line.starts_with("GET /v2/positions ") {
                            ("200 OK", positions.as_str())
                        } else if request_line.starts_with("GET /v2/account ") {
                            ("200 OK", account.as_str())
                        } else {
                            ("404 Not Found", "{}")
                        };
                        write_json_response(&mut stream, status, body);
                        requests.push(request_line);
                        idle_deadline = Instant::now() + Duration::from_secs(3);
                    }
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(10));
                }
                Err(_) => break,
            }
        }

        requests
    });

    (format!("http://{addr}"), handle)
}

fn read_request_line(stream: &mut TcpStream) -> Option<String> {
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .expect("fake server should set read timeout");
    let mut buffer = [0_u8; 4096];
    let read = stream.read(&mut buffer).ok()?;
    if read == 0 {
        return None;
    }

    let request = String::from_utf8_lossy(&buffer[..read]);
    request.lines().next().map(str::to_owned)
}

fn write_json_response(stream: &mut TcpStream, status: &str, body: &str) {
    let response = format!(
        "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    stream
        .write_all(response.as_bytes())
        .expect("fake server should write response");
}
