use metrics::{counter, gauge, value};

fn main() {
    let receiver = metrics_runtime::Receiver::builder().build().unwrap();
    let controller = receiver.controller();
    receiver.install();

    println!("Hello, world!");
    counter!("main.count", 1);
    gauge!("main.gauge", 23);
    value!("main.value", 5);
    value!("main.value", 25);
    value!("main.value", 26);
    counter!("main.count", 1);
    counter!("main.count", 3);

    // use metrics_runtime::observers::YamlBuilder;
    // let mut tty_exporter = StdoutExporter::new(controller, YamlBuilder::new());
    let mut tty_exporter = StdoutExporter::new(controller.clone(), StatsdBuilder::new());
    tty_exporter.turn();

    let socket = std::net::UdpSocket::bind("0.0.0.0:0").unwrap();
    let addr = std::net::SocketAddr::from(([127, 0, 0, 1], 1234));
    let mut udp_exporter = UdpExporter::new(controller, StatsdBuilder::new(), socket, addr);
    udp_exporter.turn().unwrap();
    counter!("main.count", 1);
    udp_exporter.turn().unwrap();
}

struct StdoutExporter<C, B>
where
    C: metrics_core::Observe,
    B: metrics_core::Builder,
{
    controller: C,
    observer: B::Output,
}

impl<C, B> StdoutExporter<C, B>
where
    B: metrics_core::Builder,
    B::Output: metrics_core::Drain<String> + metrics_core::Observer,
    C: metrics_core::Observe,
{
    fn new(controller: C, builder: B) -> Self {
        Self {
            controller,
            observer: builder.build(),
        }
    }

    fn turn(&mut self) {
        self.controller.observe(&mut self.observer);
        let output = self.observer.drain();
        println!("{}", output);
    }
}

struct UdpExporter<C, B>
where
    C: metrics_core::Observe,
    B: metrics_core::Builder,
{
    controller: C,
    observer: B::Output,
    socket: std::net::UdpSocket,
    addr: std::net::SocketAddr,
}

impl<C, B> UdpExporter<C, B>
where
    B: metrics_core::Builder,
    B::Output: metrics_core::Drain<String> + metrics_core::Observer,
    C: metrics_core::Observe,
{
    fn new(
        controller: C,
        builder: B,
        socket: std::net::UdpSocket,
        addr: std::net::SocketAddr,
    ) -> Self {
        Self {
            controller,
            observer: builder.build(),
            socket,
            addr,
        }
    }

    fn turn(&mut self) -> Result<(), std::io::Error> {
        self.controller.observe(&mut self.observer);
        let data = self.observer.drain();
        // Split to multiple packets if size > 512 bytes
        self.socket.send_to(data.as_bytes(), &self.addr)?;
        Ok(())
    }
}

use std::io::Write;

use metrics_core::Drain;

struct StatsdBuilder;

impl StatsdBuilder {
    fn new() -> Self {
        Self
    }
}

impl metrics_core::Builder for StatsdBuilder {
    type Output = StatsdObserver;

    fn build(&self) -> Self::Output {
        StatsdObserver {
            metrics: std::collections::HashMap::new(),
            prev_metrics: std::collections::HashMap::new(),
        }
    }
}

struct StatsdObserver {
    metrics: std::collections::HashMap<metrics_core::ScopedString, MetricValue>,
    prev_metrics: std::collections::HashMap<metrics_core::ScopedString, MetricValue>,
}

impl metrics_core::Observer for StatsdObserver {
    fn observe_counter(&mut self, key: metrics_core::Key, value: u64) {
        self.metrics.insert(key.name(), MetricValue::Counter(value));
    }

    fn observe_gauge(&mut self, key: metrics_core::Key, value: i64) {
        self.metrics.insert(key.name(), MetricValue::Gauge(value));
    }

    fn observe_histogram(&mut self, _key: metrics_core::Key, _value: &[u64]) {
        () // Implement this
    }
}

impl metrics_core::Drain<String> for StatsdObserver {
    fn drain(&mut self) -> String {
        // Writing to a Vec never fails.
        let mut out: Vec<u8> = Vec::new();
        for (key, value) in self.metrics.iter_mut() {
            let updated = match self.prev_metrics.get(key) {
                Some(prev_value) => value != prev_value,
                None => true,
            };
            if updated {
                match value {
                    MetricValue::Counter(val) => {
                        writeln!(out, "{key}:{val}:{typ}", key = key, val = val, typ = 'c')
                            .expect("writing to Vec");
                    }
                    MetricValue::Gauge(val) => {
                        writeln!(out, "{key}:{val}:{typ}", key = key, val = val, typ = 'g')
                            .expect("writing to Vec")
                    }
                    MetricValue::Cleared => continue,
                }
                self.prev_metrics.insert(key.clone(), value.clone());
            }
            *value = MetricValue::Cleared;
        }

        // Because we started with UTF-8 input but formatted it into bytes it is
        // safe to turn it back into UTF-8.
        String::from_utf8(out).expect("Non-UTF8 metric")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum MetricValue {
    Counter(u64),
    Gauge(i64),
    Cleared,
}
