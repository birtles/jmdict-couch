#[macro_use]
extern crate structopt;
extern crate quick_xml;
extern crate smallvec;

// TODO
// -- The error handling here is embarassing
// -- The factoring is awkward (really, pass the first tag then get function to pass the rest?)
//    How is this supposed to work?
// -- All this could use tests
// -- Check this actually works at scale (i.e. on the whole file)
// -- Check the copyright for the dictionary is actually acceptable
// -- Now that this is starting to come together, work out how better to factor out common code.
//    Macros? Mako?

use smallvec::SmallVec;
use std::path::PathBuf;
use std::str::FromStr;
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

type InfoVec = SmallVec<[String; 4]>;
type PriorityVec = SmallVec<[String; 4]>;

/// entry from jmdict schema
#[derive(Debug)]
struct Entry {
    /// ent_seq
    id: u32,
    /// k_ele children
    kanji_entries: Vec<KanjiEntry>,
    /// r_ele children
    reading_entries: Vec<ReadingEntry>,
}

/// k_ele from jmdict schema
#[derive(Debug)]
struct KanjiEntry {
    /// keb
    kanji: String,
    /// ke_inf
    info: InfoVec,
    /// ke_pri
    priority: PriorityVec,
}

/// r_ele from jmdict schema
#[derive(Debug)]
struct ReadingEntry {
    /// reb
    kana: String,
    /// re_nokanji
    no_kanji: bool,
    /// re_restr
    related_kanji: Vec<String>,
    /// re_inf
    info: InfoVec,
    /// re_pri
    priority: PriorityVec,
}

fn main() {
    let opt = Opt::from_args();

    let mut reader = Reader::from_file(opt.input).expect("Could not read from file");
    reader.trim_text(true);
    reader.check_end_names(false);
    reader.expand_empty_elements(true);

    let mut buf = Vec::new();
    let mut entries: Vec<Entry> = Vec::new();

    loop {
        match reader.read_event(&mut buf) {
            Ok(Event::Start(ref e)) => {
                match e.name() {
                    // TODO: Work out if the b here is necessary
                    b"entry" => {
                        entries.push(parse_entry(&mut reader).expect("Failed to parse entry"));
                    },
                    _ => (),
                }
            },
            Ok(Event::Eof) => break,
            Err(e) => panic!("Error at position {}: {:?}", reader.buffer_position(), e),
            _ => (),
        }
        buf.clear();
    }

    for entry in entries {
        println!("> {:?}", entry);
    }
}

fn parse_entry<T: std::io::BufRead>(reader: &mut Reader<T>) -> Result<Entry, ()> {
    let mut id: u32 = 0;
    let mut kanji_entries: Vec<KanjiEntry> = Vec::new();
    let mut reading_entries: Vec<ReadingEntry> = Vec::new();

    let mut buf = Vec::new();
    let mut ent_seq = false;

    loop {
        match reader.read_event(&mut buf) {
            Ok(Event::Start(ref e)) => {
                match e.name() {
                    b"k_ele" => kanji_entries.push(parse_k_ele(reader)?),
                    b"r_ele" => reading_entries.push(parse_r_ele(reader)?),
                    b"ent_seq" => {
                        assert!(!ent_seq);
                        ent_seq = true;
                    },
                    _ => (),
                }
            },
            Ok(Event::End(ref e)) => {
                match e.name() {
                    b"entry" => break,
                    b"ent_seq" => {
                        assert!(ent_seq);
                        ent_seq = false;
                    }
                    _ => (),
                }
            },
            Ok(Event::Text(e)) => {
                if ent_seq {
                    id = u32::from_str(&e.unescape_and_decode(&reader).unwrap()).unwrap();
                }
            },
            Err(_) => return Err(()),
            _ => (),
        }
        buf.clear();
    }

    // TODO: Is there a shorthand for checking if an array is empty?
    if id == 0 || reading_entries.len() == 0 {
        return Err(())
    }

    Ok(Entry { id, kanji_entries, reading_entries })
}

fn parse_k_ele<T: std::io::BufRead>(reader: &mut Reader<T>) -> Result<KanjiEntry, ()> {
    let mut kanji: String = String::new();
    let mut info: InfoVec = InfoVec::new();
    let mut priority: PriorityVec = PriorityVec::new();

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
                    Some(Elem::Keb) => kanji = e.unescape_and_decode(&reader).unwrap(),
                    Some(Elem::KeInf) => info.push(e.unescape_and_decode(&reader).unwrap()),
                    Some(Elem::KePri) => priority.push(e.unescape_and_decode(&reader).unwrap()),
                    _ => (), // TODO: Make this warn
                }
            },
            Err(_) => return Err(()),
            _ => (),
        }
        buf.clear();
    }

    // TODO: Check that kanji is not the empty string

    Ok(KanjiEntry { kanji, info, priority })
}

fn parse_r_ele<T: std::io::BufRead>(reader: &mut Reader<T>) -> Result<ReadingEntry, ()> {
    let mut kana = String::new();
    let mut no_kanji = false;
    let mut related_kanji: Vec<String> = Vec::new();
    let mut info: InfoVec = InfoVec::new();
    let mut priority: PriorityVec = PriorityVec::new();

    enum Elem {
        Reb,
        ReRestr,
        ReInf,
        RePri,
    }
    let mut elem: Option<Elem> = None;
    let mut buf = Vec::new();

    loop {
        match reader.read_event(&mut buf) {
            Ok(Event::Start(ref e)) => {
                match e.name() {
                    b"reb" => elem = Some(Elem::Reb),
                    b"re_nokanji" => no_kanji = true,
                    b"re_restr" => elem = Some(Elem::ReRestr),
                    b"re_inf" => elem = Some(Elem::ReInf),
                    b"re_pri" => elem = Some(Elem::RePri),
                    _ => (), // TODO: This should probably warn
                }
            },
            Ok(Event::End(ref e)) => {
                match e.name() {
                    b"r_ele" => break,
                    _ => elem = None,
                }
            },
            Ok(Event::Text(e)) => {
                match elem {
                    Some(Elem::Reb) => kana = e.unescape_and_decode(&reader).unwrap(),
                    Some(Elem::ReRestr) => related_kanji.push(e.unescape_and_decode(&reader).unwrap()),
                    Some(Elem::ReInf) => info.push(e.unescape_and_decode(&reader).unwrap()),
                    Some(Elem::RePri) => priority.push(e.unescape_and_decode(&reader).unwrap()),
                    _ => (), // TODO: Make this warn
                }
            },
            Err(_) => return Err(()),
            _ => (),
        }
        buf.clear();
    }

    // TODO: Check that kana is not the empty string

    Ok(ReadingEntry { kana, no_kanji, related_kanji, info, priority })
}
