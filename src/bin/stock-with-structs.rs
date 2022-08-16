use server::ThreadPool;
use std::fmt::Display;
use std::io::prelude::*;
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, SystemTime};

const SNAPSHOT_WINDOW_SIZE: usize = 10;

struct Metrics {
    total_requests: Arc<Mutex<i32>>,
    snapshots: Arc<Mutex<Vec<Snapshot>>>,
}
impl Metrics {
    fn increment(&self) {
        let mut count = self.total_requests.lock().unwrap();
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

fn main() {
    let addr = SocketAddr::from(([0, 0, 0, 0, 0, 0, 0, 0], 8080));
    let listener = TcpListener::bind(addr).expect("Issue binding to IP Address");
    // listener.set_ttl(100).expect("Couldn't set TTL");
    let pool = ThreadPool::new(4);
    let start_time = SystemTime::now();

    ctrlc::set_handler(move || {
        std::process::exit(0);
    })
    .expect("Error setting Ctrl-C handler");

    let metrics = Arc::new(Mutex::new(Metrics {
        total_requests: Arc::new(Mutex::new(0)),
        snapshots: Arc::new(Mutex::new(Vec::new())),
    }));

    /*
     * Last N snapshots: (total_request_count, current_time)
     *
     * then we can take an item i and j and calculate the RPS in that delta
     */

    let metrics_ref = Arc::clone(&metrics);
    /*
     * Metrics thread
     */
    thread::spawn(move || loop {
        match start_time.elapsed() {
            Ok(elapsed) => {
                if elapsed.as_millis() > 0 {
                    let metrics = metrics_ref.lock().expect("Couldn't lock metrics");

                    let mut snapshots_stats = metrics.snapshots.lock().unwrap();
                    let current_requests = metrics.total_requests.lock().unwrap();

                    let elapsed_ms = elapsed.as_millis();

                    // need 2 to compare
                    if snapshots_stats.len() > 2 {
                        let rps = get_rps_for_snapshots(
                            snapshots_stats.get(0).unwrap(),
                            snapshots_stats.get(snapshots_stats.len() - 1).unwrap(),
                        );

                        println!("RPS {}, total requests: {}", rps, *current_requests);
                    }

                    // save snapshot
                    snapshots_stats.push(Snapshot {
                        elapsed_time_millis: elapsed_ms,
                        total_request_count: *current_requests,
                    });

                    // trim the beginning of the array to retain N snapshots
                    let upper_bound_remove = snapshots_stats.len()
                        - std::cmp::min(snapshots_stats.len(), SNAPSHOT_WINDOW_SIZE);
                    snapshots_stats.drain(0..upper_bound_remove);
                }
            }
            Err(e) => {
                // ignoring
                println!("Error {}", e);
            }
        }
        thread::sleep(Duration::from_secs(1));
    });

    for stream in listener.incoming() {
        let stream = stream.unwrap();
        let metrics = Arc::clone(&metrics);

        // let snapshot_for_worker_thread = Arc::clone(&snapshots);
        // let cloned_counter_2 = Arc::clone(&request_counter);
        pool.execute(move || {
            handle_connection(stream);

            let metrics = metrics.lock().expect("Couldn't lock metrics");
            metrics.increment();
            // let mut current_counter = cloned_counter_2.lock().unwrap();
            // *current_counter += 1;
        })
    }
}

fn handle_connection(mut stream: TcpStream) {
    let mut buffer = [0; 1024];

    stream.read(&mut buffer).unwrap();

    let get = b"GET / HTTP/1.1\r\n";
    let sleep = b"GET /sleep HTTP/1.1\r\n";

    let status_line = if buffer.starts_with(get) {
        "HTTP/1.1 200 OK"
    } else if buffer.starts_with(sleep) {
        thread::sleep(Duration::from_secs(5));
        "HTTP/1.1 200 OK"
    } else {
        "HTTP/1.1 404 NOT FOUND"
    };

    let contents = String::from("Hey there");

    let response = format!(
        "{}\r\nContent-Length: {}\r\n\r\n{}",
        status_line,
        contents.len(),
        contents
    );

    stream.write(response.as_bytes()).unwrap();
    stream.flush().unwrap();
    stream.shutdown(std::net::Shutdown::Both).ok();

}
