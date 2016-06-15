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
use serde_json::Value;
use std::io::{stderr, Write, Read};
use std::path::{PathBuf, Path};
use std::sync::mpsc::channel;
use threadpool::ThreadPool;

#[derive(Clone)]
struct MyError(String);

impl From<serde_json::error::Error> for MyError {
    fn from(_err: serde_json::error::Error) -> Self {
        MyError(String::from("Failed to parse JSON reply"))
    }
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

fn parse_json(name: &Path, reply: &str) -> Result<(), MyError> {
    let err_msg = MyError(String::from("Failed to parse JSON reply"));

    let data: Value = try!(serde_json::from_str(reply));
    let obj = try!(data.as_object().ok_or(err_msg.clone()));
    let result = try!(obj.get("result").unwrap().as_object().ok_or(err_msg.clone()));
    let url = try!(result.get("url").unwrap().as_string().ok_or(err_msg.clone()));

    println!("{}: {}", name.display(), url);
    Ok(())
}

fn upload_files(files: Vec<PathBuf>, concurrent: usize, verbose: bool, json: bool) {
    if verbose {
        println!("Concurrent uploads: {}", concurrent);
    }

    let pool = ThreadPool::new(concurrent);
    let (tx, rx) = channel::<Result<(), ()>>();

    for file in &files {
        let file = file.clone();
        let tx = tx.clone();
        pool.execute(move|| {
            let _ = tx;

            if verbose {
                let _ = writeln!(stderr(), "Uploading {}", &file.display());
            }

            let request =
                Request::new(Method::Post,
                Url::parse("https://api.teknik.io/v1/Upload").unwrap())
                .unwrap();

            let mut multipart = Multipart::from_request(request).unwrap();

            let _ = multipart.write_file("file", &file).unwrap();
            let mut response: Response = multipart.send().unwrap();

            let mut reply = String::new();
            let _ = response.read_to_string(&mut reply).unwrap();

            if let StatusCode::Ok = response.status {
                if json {
                    println!("{}", reply);
                } else {
                    let _ = parse_json(&file, &reply);
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
        .about("Uploads files to u.teknik.io")
        .version("0.1.0")
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
             .help("Output full JSON reply rather than just image URL"))
        .get_matches();

    let files_vec = matches.values_of_os("file")
        .unwrap()
        .map(PathBuf::from)
        .collect::<Vec<_>>();

    let concurrent_uploads: usize = matches.value_of("concurrent")
        .and_then(|s| s.parse().ok())
        .unwrap_or(num_cpus::get());

    let verbose = matches.is_present("verbose");

    let json = matches.is_present("json");

    upload_files(files_vec, concurrent_uploads, verbose, json);
}
