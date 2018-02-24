#[macro_use]
extern crate failure;
extern crate quick_xml;
extern crate smallvec;
#[macro_use]
extern crate structopt;

// TODO
// -- The factoring is awkward (really, pass the first tag then get function to pass the rest?)
//    How is this supposed to work?
// -- All this could use tests
// -- Now that this is starting to come together, work out how better to factor out common code.
//    Macros? Mako?

use failure::{Error, ResultExt};
use smallvec::SmallVec;
use std::path::PathBuf;
use std::str;
use std::str::FromStr;
use structopt::StructOpt;
use quick_xml::reader::Reader;
use quick_xml::events::{BytesText, Event};

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
    /// sense children
    senses: Vec<Sense>,
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

/// sense from jmdict schema
#[derive(Debug)]
struct Sense {
    /// stagk
    only_kanji: Vec<String>,
    /// stagr
    only_readings: Vec<String>,
    // xref
    // cross_refs: Vec<CrossReference>,
    // ant
    // XXX Are there ever more than one of these?
    // antonyms: Vec<String>,
    // pos
    // XXX Need enum for this?
    // info: PartOfSpeech,
    // field
    // XXX Use enum for this
    // field: String,
    // l_source
    // lang_sources: Vec<LangSource>,
    // dial
    // dialect: Option<String>,
    // gloss
    // glosses: Vec<Gloss>,
}

/*
struct CrossReference {
    kanji_or_reading: String,
    sense_index: Option<u8>,
}
*/

fn main() {
    let opt = Opt::from_args();

    let entries = get_entries(&opt.input);
    if let Err(ref e) = entries {
        use std::io::Write;
        let stderr = &mut ::std::io::stderr();
        writeln!(stderr, "{}", e).expect("Error writing to stderr");
        ::std::process::exit(1);
    }

    let entries = entries.unwrap();

    for entry in entries {
        println!("> {:?}", entry);
    }
}

fn get_entries(input: &PathBuf) -> Result<Vec<Entry>, Error> {
    let mut reader = Reader::from_file(input).context("Could not read from file")?;
    reader.trim_text(true);
    reader.check_end_names(false);
    reader.expand_empty_elements(true);

    let mut buf = Vec::new();
    let mut entries: Vec<Entry> = Vec::new();

    loop {
        match reader.read_event(&mut buf) {
            Ok(Event::Start(ref e)) => match e.name() {
                b"entry" => {
                    entries.push(parse_entry(&mut reader)?);
                }
                _ => (),
            },
            Ok(Event::Eof) => break,
            Err(e) => bail!(
                "Error parsing entry at position #{}: {}",
                reader.buffer_position(),
                e
            ),
            _ => (),
        }
        buf.clear();
    }

    Ok(entries)
}

fn parse_entry<T: std::io::BufRead>(reader: &mut Reader<T>) -> Result<Entry, Error> {
    let mut id: u32 = 0;
    let mut kanji_entries: Vec<KanjiEntry> = Vec::new();
    let mut reading_entries: Vec<ReadingEntry> = Vec::new();
    let mut senses: Vec<Sense> = Vec::new();

    let mut buf = Vec::new();
    let mut ent_seq = false;

    loop {
        match reader.read_event(&mut buf) {
            Ok(Event::Start(ref e)) => match e.name() {
                b"ent_seq" => {
                    ensure!(
                        !ent_seq,
                        "Nested ent_seq at position #{}",
                        reader.buffer_position()
                    );
                    ent_seq = true;
                }
                b"k_ele" => kanji_entries.push(parse_k_ele(reader)?),
                b"r_ele" => reading_entries.push(parse_r_ele(reader)?),
                b"sense" => senses.push(parse_sense(reader)?),
                _ => warn_unknown_tag(e.name(), reader.buffer_position(), "entry"),
            },
            Ok(Event::End(ref e)) => match e.name() {
                b"entry" => break,
                b"ent_seq" => {
                    ensure!(
                        ent_seq,
                        "Mismatched ent_seq tags at position #{}",
                        reader.buffer_position()
                    );
                    ent_seq = false;
                }
                _ => (),
            },
            Ok(Event::Text(e)) => {
                if ent_seq {
                    id = u32::from_str(&e.unescape_and_decode(&reader)?)
                        .context("Failed to parse ent_seq as int")?;
                }
            }
            Err(e) => bail!(
                "Error parsing entry at position #{}: {}",
                reader.buffer_position(),
                e
            ),
            _ => (),
        }
        buf.clear();
    }

    ensure!(
        id != 0,
        "ID not found at position #{}",
        reader.buffer_position()
    );
    ensure!(
        !reading_entries.is_empty(),
        "No reading entries found at position #{}",
        reader.buffer_position()
    );

    Ok(Entry {
        id,
        kanji_entries,
        reading_entries,
        senses,
    })
}

fn parse_k_ele<T: std::io::BufRead>(reader: &mut Reader<T>) -> Result<KanjiEntry, Error> {
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
            Ok(Event::Start(ref e)) => match e.name() {
                b"keb" => elem = Some(Elem::Keb),
                b"ke_inf" => elem = Some(Elem::KeInf),
                b"ke_pri" => elem = Some(Elem::KePri),
                _ => warn_unknown_tag(e.name(), reader.buffer_position(), "k_ele"),
            },
            Ok(Event::End(ref e)) => match e.name() {
                b"k_ele" => break,
                _ => elem = None,
            },
            Ok(Event::Text(e)) => match elem {
                Some(Elem::Keb) => kanji = e.unescape_and_decode(&reader)?,
                Some(Elem::KeInf) => info.push(e.unescape_and_decode(&reader)?),
                Some(Elem::KePri) => priority.push(e.unescape_and_decode(&reader)?),
                _ => warn_unexpected_text(&e, reader, "k_ele"),
            },
            Err(e) => bail!(
                "Error parsing entry at position #{}: {}",
                reader.buffer_position(),
                e
            ),
            _ => (),
        }
        buf.clear();
    }

    assert!(
        kanji.trim() == kanji,
        "Kanji keys should not have leading or trailing whitespace"
    );
    ensure!(
        !kanji.is_empty(),
        "Kanji key is empty at position #{}",
        reader.buffer_position()
    );

    Ok(KanjiEntry {
        kanji,
        info,
        priority,
    })
}

fn parse_r_ele<T: std::io::BufRead>(reader: &mut Reader<T>) -> Result<ReadingEntry, Error> {
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
            Ok(Event::Start(ref e)) => match e.name() {
                b"reb" => elem = Some(Elem::Reb),
                b"re_nokanji" => no_kanji = true,
                b"re_restr" => elem = Some(Elem::ReRestr),
                b"re_inf" => elem = Some(Elem::ReInf),
                b"re_pri" => elem = Some(Elem::RePri),
                _ => warn_unknown_tag(e.name(), reader.buffer_position(), "r_ele"),
            },
            Ok(Event::End(ref e)) => match e.name() {
                b"r_ele" => break,
                _ => elem = None,
            },
            Ok(Event::Text(e)) => match elem {
                Some(Elem::Reb) => kana = e.unescape_and_decode(&reader).unwrap(),
                Some(Elem::ReRestr) => related_kanji.push(e.unescape_and_decode(&reader).unwrap()),
                Some(Elem::ReInf) => info.push(e.unescape_and_decode(&reader).unwrap()),
                Some(Elem::RePri) => priority.push(e.unescape_and_decode(&reader).unwrap()),
                _ => warn_unexpected_text(&e, reader, "r_ele"),
            },
            Err(e) => bail!(
                "Error parsing entry at position #{}: {}",
                reader.buffer_position(),
                e
            ),
            _ => (),
        }
        buf.clear();
    }

    assert!(
        kana.trim() == kana,
        "Kana keys should not have leading or trailing whitespace"
    );
    ensure!(
        !kana.is_empty(),
        "Kana key is empty at position #{}",
        reader.buffer_position()
    );

    Ok(ReadingEntry {
        kana,
        no_kanji,
        related_kanji,
        info,
        priority,
    })
}

fn parse_sense<T: std::io::BufRead>(reader: &mut Reader<T>) -> Result<Sense, Error> {
    let mut only_kanji: Vec<String> = Vec::new();
    let mut only_readings: Vec<String> = Vec::new();

    enum Elem {
        SenseTagKanji,
        SenseTagReading,
    }
    let mut elem: Option<Elem> = None;
    let mut buf = Vec::new();

    loop {
        match reader.read_event(&mut buf) {
            Ok(Event::Start(ref e)) => match e.name() {
                b"stagk" => elem = Some(Elem::SenseTagKanji),
                b"stagr" => elem = Some(Elem::SenseTagReading),
                // _ => warn_unknown_tag(e.name(), reader.buffer_position(), "r_ele"),
                _ => (),
            },
            Ok(Event::End(ref e)) => match e.name() {
                b"sense" => break,
                _ => elem = None,
            },
            Ok(Event::Text(e)) => match elem {
                Some(Elem::SenseTagKanji) => {
                    only_kanji.push(e.unescape_and_decode(&reader).unwrap())
                }
                Some(Elem::SenseTagReading) => {
                    only_readings.push(e.unescape_and_decode(&reader).unwrap())
                }
                // _ => warn_unexpected_text(&e, reader, "r_ele"),
                _ => (),
            },
            Err(e) => bail!(
                "Error parsing entry at position #{}: {}",
                reader.buffer_position(),
                e
            ),
            _ => (),
        }
        buf.clear();
    }

    Ok(Sense {
        only_kanji,
        only_readings,
    })
}

fn warn_unknown_tag(elem_name: &[u8], buffer_position: usize, ancestor: &str) {
    match str::from_utf8(elem_name) {
        Ok(tag) => println!(
            "WARNING: Unrecognized {} member element {} at position #{}",
            ancestor, tag, buffer_position
        ),
        _ => println!(
            "WARNING: Unrecognized {} member element (non-utf8) at position #{}",
            ancestor, buffer_position
        ),
    }
}

fn warn_unexpected_text<T: std::io::BufRead>(text: &BytesText, reader: &Reader<T>, ancestor: &str) {
    match text.unescape_and_decode(reader) {
        Ok(text) => println!(
            "WARNING: Unexpected text \"{}\" in {} element at position #{}",
            text,
            ancestor,
            reader.buffer_position(),
        ),
        _ => println!(
            "WARNING: Unexpected text in {} element (non-utf8) at position #{}",
            ancestor,
            reader.buffer_position()
        ),
    }
}
