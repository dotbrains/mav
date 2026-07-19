use std::sync::{Arc, Mutex};

use gpui::Context;

use super::*;

/// The state of the mock network.
struct MockNetworkState {
    ordering: Vec<i32>,
    a_to_b: Vec<i32>,
    b_to_a: Vec<i32>,
}

/// A mock network that delivers messages between two peers.
#[derive(Clone)]
struct MockNetwork {
    state: Arc<Mutex<MockNetworkState>>,
}

impl MockNetwork {
    fn new() -> Self {
        Self {
            state: Arc::new(Mutex::new(MockNetworkState {
                ordering: Vec::new(),
                a_to_b: Vec::new(),
                b_to_a: Vec::new(),
            })),
        }
    }

    fn a_client(&self) -> NetworkClient {
        NetworkClient {
            network: self.clone(),
            is_a: true,
        }
    }

    fn b_client(&self) -> NetworkClient {
        NetworkClient {
            network: self.clone(),
            is_a: false,
        }
    }
}

/// A client handle for sending/receiving messages over the mock network.
#[derive(Clone)]
struct NetworkClient {
    network: MockNetwork,
    is_a: bool,
}

// See, networking is easy!
impl NetworkClient {
    fn send(&self, value: i32) {
        let mut network = self.network.state.lock().unwrap();
        network.ordering.push(value);
        if self.is_a {
            network.b_to_a.push(value);
        } else {
            network.a_to_b.push(value);
        }
    }

    fn receive_all(&self) -> Vec<i32> {
        let mut network = self.network.state.lock().unwrap();
        if self.is_a {
            network.a_to_b.drain(..).collect()
        } else {
            network.b_to_a.drain(..).collect()
        }
    }
}

/// A networked counter that can send/receive over a mock network.
struct NetworkedCounter {
    count: i32,
    client: NetworkClient,
}

impl NetworkedCounter {
    fn new(client: NetworkClient) -> Self {
        Self { count: 0, client }
    }

    /// Increment the counter and broadcast the change.
    fn increment(&mut self, delta: i32, cx: &mut Context<Self>) {
        self.count += delta;

        cx.background_spawn({
            let client = self.client.clone();
            async move {
                client.send(delta);
            }
        })
        .detach();
    }

    /// Process incoming increment requests.
    fn sync(&mut self) {
        for delta in self.client.receive_all() {
            self.count += delta;
        }
    }
}

/// You can simulate distributed systems with multiple app contexts, simply by adding
/// additional parameters.
#[gpui::test]
fn test_app_sync(cx_a: &mut TestAppContext, cx_b: &mut TestAppContext) {
    let network = MockNetwork::new();

    let a = cx_a.new(|_| NetworkedCounter::new(network.a_client()));
    let b = cx_b.new(|_| NetworkedCounter::new(network.b_client()));

    // B increments locally and broadcasts the delta
    b.update(cx_b, |b, cx| b.increment(42, cx));
    b.read_with(cx_b, |b, _| assert_eq!(b.count, 42)); // B's count is set immediately
    a.read_with(cx_a, |a, _| assert_eq!(a.count, 0)); // A's count is in a side effect

    cx_b.run_until_parked(); // Send the delta from B
    a.update(cx_a, |a, _| a.sync()); // Receive the delta at A

    b.read_with(cx_b, |b, _| assert_eq!(b.count, 42)); // Both counts now match
    a.read_with(cx_a, |a, _| assert_eq!(a.count, 42));
}

/// Multiple apps can run concurrently, and to capture this each test app shares
/// a dispatcher. Whenever you call `run_until_parked`, the dispatcher will randomly
/// pick which app's tasks to run next. This allows you to test that your distributed code
/// is robust to different execution orderings.
#[gpui::test(iterations = 10)]
fn test_random_interleaving(cx_a: &mut TestAppContext, cx_b: &mut TestAppContext, mut rng: StdRng) {
    let network = MockNetwork::new();

    // Track execution order
    let mut original_order = Vec::new();
    let a = cx_a.new(|_| NetworkedCounter::new(MockNetwork::a_client(&network)));
    let b = cx_b.new(|_| NetworkedCounter::new(MockNetwork::b_client(&network)));

    let num_operations: usize = rng.random_range(3..8);

    for i in 0..num_operations {
        let i = i as i32;
        let which = rng.random_bool(0.5);

        original_order.push(i);
        if which {
            b.update(cx_b, |b, cx| b.increment(i, cx));
        } else {
            a.update(cx_a, |a, cx| a.increment(i, cx));
        }
    }

    // This will send all of the pending increment messages, from both a and b
    cx_a.run_until_parked();

    a.update(cx_a, |a, _| a.sync());
    b.update(cx_b, |b, _| b.sync());

    let a_count = a.read_with(cx_a, |a, _| a.count);
    let b_count = b.read_with(cx_b, |b, _| b.count);

    assert_eq!(a_count, b_count, "A and B should have the same count");

    // Nicely format the execution order output.
    // Run this test with `-- --nocapture` to see it!
    let actual = network.state.lock().unwrap().ordering.clone();
    let spawned: Vec<_> = original_order.iter().map(|n| format!("{}", n)).collect();
    let ran: Vec<_> = actual.iter().map(|n| format!("{}", n)).collect();
    let diff: Vec<_> = original_order
        .iter()
        .zip(actual.iter())
        .map(|(o, a)| if o == a { " " } else { "^" }.to_string())
        .collect();
    println!("spawned: [{}]", spawned.join(", "));
    println!("ran:     [{}]", ran.join(", "));
    println!("         [{}]", diff.join(", "));
}
