#[macro_use]
extern crate structopt;
extern crate quick_xml;

use std::path::PathBuf;
use structopt::StructOpt;
use quick_xml::reader::Reader;
use quick_xml::events::Event;

#[derive(StructOpt)]
#[structopt(name = "jmdict-couch")]
/// Perform an incremental update of a CouchDB representation of the JMDict database using the
/// supplied JMDict XML file.
struct Opt {
    #[structopt(short = "i", long = "input", help = "Input file", parse(from_os_str))]
    input: PathBuf,
}

fn main() {
    let opt = Opt::from_args();

    // TODO: Support reading from stdin?

    let mut reader = Reader::from_file(opt.input).expect("Could not read from file");
    reader.trim_text(true);
    reader.check_end_names(false);

    let mut txt = Vec::new();
    let mut buf = Vec::new();

    // TODO: This is currently getting caught up on the entity references. We could:
    // - Pre-parse the input using xmllint to expand the entity references
    // - Parse just the element text ourselves and escape them (could be really hard?)
    // - Try to parse all the elements in the DOCTYPE and actually do the substitution (super hard!)

    loop {
        match reader.read_event(&mut buf) {
            Ok(Event::Start(ref e)) => {
                match e.name() {
                    b"entry" => println!("entry"),
                    _ => (),
                }
            },
            Ok(Event::Text(e)) => txt.push(e.unescape_and_decode(&reader).unwrap()),
            Ok(Event::Eof) => break,
            Err(e) => panic!("Error at position {}: {:?}", reader.buffer_position(), e),
            _ => (),
        }

        // If we don't keep a borrow elsewhere, we can clear the buffer to keep memory usage low
        buf.clear();
    }

    for line in txt {
        println!("> {}", line);
    }
}
