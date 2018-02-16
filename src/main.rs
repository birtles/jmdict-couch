#[macro_use]
extern crate structopt;
extern crate xml;

use std::fs::File;
use std::io::BufReader;
use std::path::PathBuf;
use structopt::StructOpt;
use xml::reader::{EventReader, ParserConfig, XmlEvent};

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

    let file = File::open(opt.input).expect("Could not read from file");
    let file = BufReader::new(file);

    let mut in_entry = false;

    let parser = EventReader::new_with_config(file,
                                              ParserConfig::new()
                                              .ignore_comments(false)
                                              .whitespace_to_characters(true)
                                              .trim_whitespace(true));
    for e in parser {
        match e {
            Ok(XmlEvent::StartElement { name, .. }) => {
                if name.local_name == "entry" {
                    assert!(!in_entry, "Entries should not be nested");
                    in_entry = true;
                    println!("{}", name);
                }
            }
            Ok(XmlEvent::EndElement { name }) => {
                if name.local_name == "entry" {
                    assert!(in_entry, "Entry tag mismatch");
                    in_entry = false;
                }
            }
            Ok(XmlEvent::Characters(text)) => {
                println!("# {}", text);
            }
            Err(e) => {
                println!("Error: {}", e);
            }
            _ => {}
        }
    }
}
