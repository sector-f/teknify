extern crate clap;
extern crate hyper;
extern crate multipart;
extern crate num_cpus;
extern crate serde_json;
extern crate threadpool;
extern crate url;

use clap::{App, Arg};
use hyper::method::Method;
use hyper::client::request::Request;
use hyper::client::response::Response;
use hyper::status::StatusCode;
use hyper::Url;
use multipart::client::Multipart;
use serde_json::error::Error;
use serde_json::Value;
use std::fmt;
use std::io::{stderr, Write, Read};
use std::path::{PathBuf, Path};
use std::sync::mpsc::channel;
use threadpool::ThreadPool;

struct TeknifyError(String);

impl fmt::Display for TeknifyError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<serde_json::error::Error> for TeknifyError {
    fn from(err: serde_json::error::Error) -> Self {
        match err {
            Error::Syntax(syn_err, _, _) => TeknifyError(format!("Syntax error: {:?}", syn_err)),
            Error::Io(io_err) => TeknifyError(format!("IO error: {}", io_err)),
            Error::FromUtf8(utf_err) => TeknifyError(format!("UTF-8 error: {}", utf_err)),
        }
    }
}

#[derive(Clone, Debug)]
enum Output {
    Json,
    NameAndUrl,
    Url,
}

fn is_positive_int(n: String) -> Result<(), String> {
    match n.parse::<usize>() {
        Ok(val) => {
            if val == 0 {
                Err(String::from("CONCURRENT UPLOADS cannot be zero"))
            } else {
                Ok(())
            }
        },
        Err(_) => Err(String::from("CONCURRENT UPLOADS must be a positive integer")),
    }
}

fn parse_json(name: &Path, reply: &str, showname: bool) -> Result<(), TeknifyError> {
    let reply_as_value: Value = try!(serde_json::from_str(reply));
    let reply_as_object = try!(reply_as_value.as_object().ok_or(TeknifyError(String::from("Response was not valid JSON"))));
    let result_section = try!(reply_as_object.get("result").ok_or(TeknifyError(String::from("Response did not contain a \"result\" section"))));
    let result_as_object = try!(result_section.as_object().ok_or(TeknifyError(String::from("Response's \"result\" section was not a valid JSON object"))));
    let url_section = try!(result_as_object.get("url").ok_or(TeknifyError(String::from("Response's \"result\" section did not contain a \"url\" section"))));
    let url = try!(url_section.as_string().ok_or(TeknifyError(String::from("Response's \"url\" section was not a valid JSON string"))));

    match showname {
        true => println!("{}: {}", name.display(), url),
        false => println!("{}", url),
    }

    Ok(())
}

fn upload_files(files: Vec<PathBuf>, concurrent: usize, verbose: bool, output_mode: Output) {
    if verbose {
        println!("Concurrent uploads: {}", concurrent);
    }

    let pool = ThreadPool::new(concurrent);
    let (tx, rx) = channel::<Result<(), ()>>();

    for file in &files {
        let file = file.clone();
        let tx = tx.clone();
        let output_mode = output_mode.clone();

        pool.execute(move|| {
            let _ = tx;

            if verbose {
                let _ = writeln!(stderr(), "Uploading {}", &file.display());
            }

            let request =
                Request::new(Method::Post,
                Url::parse("https://api.teknik.io/v1/Upload").expect("Failed to parse \"https://api.teknik.io/v1/Upload\" as valid URL"))
                .expect("Failed to generated http request");

            let mut multipart = Multipart::from_request(request).unwrap();

            let _ = multipart.write_file("file", &file).unwrap();
            let mut response: Response = multipart.send().unwrap();

            let mut reply = String::new();
            let _ = response.read_to_string(&mut reply).unwrap();

            if let StatusCode::Ok = response.status {
                match output_mode {
                    Output::Json => println!("{}", reply),
                    Output::NameAndUrl => {
                        let parse_status = parse_json(&file, &reply, true);
                        if let Err(e) = parse_status {
                            let _ = writeln!(stderr(), "Error parsing reply for {}: {}", file.display(), e);
                        }
                    },
                    Output::Url => {
                        let parse_status = parse_json(&file, &reply, false);
                        if let Err(e) = parse_status {
                            let _ = writeln!(stderr(), "Error parsing reply for {}: {}", file.display(), e);
                        }
                    },
                }
            }
        });
    }
    drop(tx);

    let mut counter: usize = 0;
    while counter < files.len() {
        let _ = rx.recv();
        counter += 1;
    }
}

fn main() {
    let matches = App::new("teknify")
        .about("Uploads files to https://u.teknik.io")
        .version("0.2.0")
        .arg(Arg::with_name("file")
             .help("The file(s) that you would like to upload")
             .index(1)
             .multiple(true)
             .required(true))
        .arg(Arg::with_name("verbose")
             .help("Print extra information")
             .short("v")
             .long("verbose"))
        .arg(Arg::with_name("concurrent")
             .short("c")
             .long("concurrent")
             .validator(is_positive_int)
             .value_name("CONCURRENT UPLOADS")
             .help("Sets the number of concurrent uploads. The default is equal to the number of CPU processors of the current machine")
             .takes_value(true))
        .arg(Arg::with_name("json")
             .short("j")
             .long("json")
             .help("Output JSON reply rather than parse it"))
        .arg(Arg::with_name("url")
             .short("u")
             .long("url")
             .help("Output only the URL rather than the filename and the URL")
             .conflicts_with("json"))
        .get_matches();

    let files_vec = matches.values_of_os("file")
        .unwrap()
        .map(PathBuf::from)
        .collect::<Vec<_>>();

    let concurrent_uploads: usize = matches.value_of("concurrent")
        .and_then(|s| s.parse().ok())
        .unwrap_or(num_cpus::get());

    let verbose = matches.is_present("verbose");

    let output_mode = {
        if ! matches.is_present("url") && ! matches.is_present("json") {
            Output::NameAndUrl // Default (no flags given)
        } else {
            if matches.is_present("url") {
                Output::Url
            } else if matches.is_present("json") {
                Output::Json
            } else {
                unreachable!()
            }
        }
    };

    upload_files(files_vec, concurrent_uploads, verbose, output_mode);
}
