extern crate curl;
extern crate tokio_core;
extern crate tokio_curl;

use std::io::{self, Write};

use curl::easy::Easy;
use tokio_core::reactor::Core;
use tokio_curl::Session;

fn main() {
    let mut core = Core::new().unwrap();
    let session = Session::new(core.handle());

    let mut request = Easy::new();
    request.get(true).unwrap();
    request.url("https://www.rust-lang.org").unwrap();
    request.write_function(|data| {
        io::stdout().write_all(data).unwrap();
        Ok(data.len())
    }).unwrap();

    let request_future = session.perform(request);

    let mut request = core.run(request_future).unwrap();
    println!("{:?}", request.response_code());
}
