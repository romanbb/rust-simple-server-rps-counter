use server::ThreadPool;
use std::fmt::Display;
use std::io::prelude::*;
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, SystemTime};

struct Snapshot {
    total_request_count: u128,
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
        / (newer.elapsed_time_millis - older.elapsed_time_millis) as f64 * 1000 as f64;
}

fn main() {
    let listener = TcpListener::bind("127.0.0.1:7878").unwrap();
    let request_counter = Arc::new(Mutex::new(0));
    let snapshots: Arc<Mutex<Vec<Snapshot>>> = Arc::new(Mutex::new(Vec::new()));
    let start = SystemTime::now();
    let pool = ThreadPool::new(4);

    /*
     * Last N snapshots: (total_request_count, current_time)
     *
     * then we can take an item i and j and calculate the RPS in that delta
     */

    let rps_counter = Arc::clone(&request_counter);
    let snapshots_for_stats_thread = Arc::clone(&snapshots);

    /*
     * Metrics thread
     */
    thread::spawn(move || loop {
        match start.elapsed() {
            Ok(elapsed) => {
                if elapsed.as_millis() > 0 {
                    let mut snapshots_stats = snapshots_for_stats_thread.lock().unwrap();
                    let current_requests = rps_counter.lock().unwrap();

                    // for i in 1..snapshots_stats.len() {
                    //     let rps_slice = get_rps_for_snapshots(snapshots_stats.get(i-1).unwrap(), snapshots_stats.get(i).unwrap());
                    //     println!("RPS slice is {}", rps_slice);
                    // }

                    let elapsed_ms = start.elapsed().ok().unwrap().as_millis();
                    if snapshots_stats.len() > 2 {
                        let rps = get_rps_for_snapshots(
                            snapshots_stats.get(0).unwrap(),
                            snapshots_stats.get(snapshots_stats.len() - 1).unwrap(),
                        );

                        // let rps = *current_requests / elapsed_ms;
                        println!("RPS {}, total requests: {}", rps, *current_requests);
                    }
                    snapshots_stats.push(Snapshot {
                        elapsed_time_millis: elapsed_ms,
                        total_request_count: *current_requests,
                    });

                    const SNAPSHOT_WINDOW_SIZE: usize = 10;

                    // 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14
                    // trim to 10
                    // if (arr.len < SNAPSHOT_WINDOW_SIZE)  do nothing
                    // else len - SNAPSHOT_WINDOW_SIZE = 14-10 = 4
                    // remove 0-4

                    let upper_bound_remove = snapshots_stats.len()
                        - std::cmp::min(snapshots_stats.len(), SNAPSHOT_WINDOW_SIZE);
                    // trim the beginning of the array to retain N snapshots
                    snapshots_stats.drain(0..upper_bound_remove);
                    
                    // print!("Snapshot stats: [");
                    // for i in snapshots_stats.iter() {
                    //     print!("{}", i);
                    // }
                    // println!("]")
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

        // let snapshot_for_worker_thread = Arc::clone(&snapshots);
        let cloned_counter_2 = Arc::clone(&request_counter);
        pool.execute(move || {
            handle_connection(stream);
            let mut current_counter = cloned_counter_2.lock().unwrap();
            *current_counter += 1;
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

    let response = format!("{}", status_line,);

    stream.write(response.as_bytes()).unwrap();
    stream.flush().unwrap();
}
