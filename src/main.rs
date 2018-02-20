#[macro_use]
extern crate structopt;
extern crate quick_xml;
// extern crate smallvec;

// TODO: Add this to Cargo.toml (and extern crate)
// use smallvec::SmallVec;
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

/// k_ele from jmdict schema
#[derive(Debug)]
struct KanjiEntry {
    /// keb
    word: String,
    /// ke_inf
    // TODO Use SmallVec below
    orthography: Vec<String>,
    /// ke_pri
    priority: Vec<String>,
}

fn main() {
    let opt = Opt::from_args();

    // TODO: Support reading from stdin?

    let mut reader = Reader::from_file(opt.input).expect("Could not read from file");
    reader.trim_text(true);
    reader.check_end_names(false);

    let mut buf = Vec::new();
    let mut kanji_entries = Vec::<KanjiEntry>::new();

    loop {
        match reader.read_event(&mut buf) {
            Ok(Event::Start(ref e)) => {
                match e.name() {
                    b"k_ele" => {
                        kanji_entries.push(parse_k_ele(&mut reader).expect("Failed to parse kanji entry"));
                    },
                    _ => (),
                }
            },
            Ok(Event::Eof) => break,
            Err(e) => panic!("Error at position {}: {:?}", reader.buffer_position(), e),
            _ => (),
        }

        // If we don't keep a borrow elsewhere, we can clear the buffer to keep memory usage low
        buf.clear();
    }

    for kanji in kanji_entries {
        println!("> {:?}", kanji);
    }
}

fn parse_k_ele<T: std::io::BufRead>(reader: &mut Reader<T>) -> Result<KanjiEntry, ()> {
    let mut word: String = String::new();
    let mut orthography: Vec<String> = Vec::new();
    let mut priority: Vec<String> = Vec::new();

    enum Elem {
        Keb,
        KeInf,
        KePri,
    }
    let mut elem: Option<Elem> = None;
    let mut buf = Vec::new();

    loop {
        match reader.read_event(&mut buf) {
            Ok(Event::Start(ref e)) => {
                match e.name() {
                    b"keb" => elem = Some(Elem::Keb),
                    b"ke_inf" => elem = Some(Elem::KeInf),
                    b"ke_pri" => elem = Some(Elem::KePri),
                    _ => (),
                }
            },
            Ok(Event::End(ref e)) => {
                match e.name() {
                    b"k_ele" => break,
                    _ => elem = None,
                }
            },
            Ok(Event::Text(e)) => {
                match elem {
                    Some(Elem::Keb) => word = e.unescape_and_decode(&reader).unwrap(),
                    Some(Elem::KeInf) => orthography.push(e.unescape_and_decode(&reader).unwrap()),
                    Some(Elem::KePri) => priority.push(e.unescape_and_decode(&reader).unwrap()),
                    _ => return Err(()),
                }
            },
            Err(_) => return Err(()),
            _ => (),
        }
        buf.clear();
    }

    Ok(KanjiEntry { word, orthography, priority })
}
