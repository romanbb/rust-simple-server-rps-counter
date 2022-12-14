use async_std::io::{ReadExt, WriteExt};
use async_std::net::{TcpListener, TcpStream};
use async_std::sync::Mutex;
use async_std::prelude::*;
use async_std::task;
use std::error::Error;
use std::fmt::Display;
use std::sync::Arc;
use std::time::{Duration, SystemTime};
const SNAPSHOT_WINDOW_SIZE: usize = 10;

struct Metrics {
    start_time: Arc<SystemTime>,
    total_requests: Arc<Mutex<i32>>,
    snapshots: Arc<Mutex<Vec<Snapshot>>>,
}
impl Metrics {
    async fn increment(&self) {
        let mut count = self.total_requests.lock().await;
        *count += 1;
    }
}
struct Snapshot {
    total_request_count: i32,
    elapsed_time_millis: u128,
}

impl Display for Snapshot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "[ total_request_count: {}, elapsed_time_millis: {} ], ",
            self.total_request_count, self.elapsed_time_millis
        )
    }
}

fn get_rps_for_snapshots(older: &Snapshot, newer: &Snapshot) -> f64 {
    if newer.total_request_count == older.total_request_count {
        return 0.0;
    }
    return (newer.total_request_count - older.total_request_count) as f64
        / (newer.elapsed_time_millis - older.elapsed_time_millis) as f64
        * 1000 as f64;
}

#[async_std::main]
async fn main() -> Result<(), Box<dyn Error>> {
    // listener.set_ttl(100).expect("Couldn't set TTL");

    let metrics = Arc::new(Mutex::new(Metrics {
        total_requests: Arc::new(Mutex::new(0)),
        start_time: Arc::new(SystemTime::now()),
        snapshots: Arc::new(Mutex::new(Vec::new())),
    }));

    ctrlc::set_handler(move || {
        std::process::exit(0);
    })
    .expect("Error setting Ctrl-C handler");

    /*
     * Last N snapshots: (total_request_count, current_time)
     *
     * then we can take an item i and j and calculate the RPS in that delta
     */

    /*
     * Metrics thread
     */

    let metrics_ref = Arc::clone(&metrics);
    task::spawn(async move {
        loop {
            let metrics = metrics_ref.lock().await;
            match metrics.start_time.elapsed() {
                Ok(elapsed) => {
                    if elapsed.as_millis() > 0 {
                        let mut snapshots = metrics.snapshots.lock().await;
                        let total_requests = metrics.total_requests.lock().await;

                        // need 2 to compare
                        if snapshots.len() > 2 {
                            let rps = get_rps_for_snapshots(
                                snapshots.get(0).unwrap(),
                                snapshots.get(snapshots.len() - 1).unwrap(),
                            );

                            println!("RPS {}, total requests: {}", rps, total_requests);
                        }

                        // save snapshot
                        snapshots.push(Snapshot {
                            elapsed_time_millis: elapsed.as_millis(),
                            total_request_count: *total_requests,
                        });

                        // trim the beginning of the array to retain N snapshots
                        let upper_bound_remove =
                            snapshots.len() - std::cmp::min(snapshots.len(), SNAPSHOT_WINDOW_SIZE);
                        snapshots.drain(0..upper_bound_remove);
                    }
                }
                Err(e) => {
                    // ignoring
                    println!("Error {}", e);
                }
            }
            task::sleep(Duration::from_secs(1)).await;
        }
    });

    let listener = TcpListener::bind("0.0.0.0:8080").await?;

    loop {
        let (socket, _) = listener.accept().await?;

        let metrics = Arc::clone(&metrics);
        task::spawn(async move {
            handle_connection(socket).await;
            let metrics = metrics.lock().await;
            metrics.increment().await;
        });
    }
}
async fn handle_connection(mut socket: TcpStream) {
    let mut buf = vec![0; 1024];
    let _read = socket.read_exact(&mut buf);

    let get = b"GET / HTTP/1.1\r\n";
    let sleep = b"GET /sleep HTTP/1.1\r\n";

    let status_line = if buf.starts_with(get) {
        "HTTP/1.1 200 OK"
    } else if buf.starts_with(sleep) {
        task::sleep(Duration::from_secs(5)).await;
        "HTTP/1.1 200 OK"
    } else {
        "HTTP/1.1 404 NOT FOUND"
    };
    // In a loop, read data from the socket and write the data back.
    // loop {
    // let n = socket
    //     .read(&mut buf)
    //     .await
    // .expect("failed to read data from socket");

    // if n == 0 {
    //     return;
    // }
    // }
    let contents = String::from("Hey there");

    let response = format!(
        "{}\r\nContent-Length: {}\r\n\r\n{}",
        status_line,
        contents.len(),
        contents
    );

    socket
        .write_all(response.as_bytes())
        .await
        .expect("failed to write data to socket");

    socket.shutdown(std::net::Shutdown::Both).ok();
}
