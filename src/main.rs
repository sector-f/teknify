extern crate clap;
extern crate hyper;
extern crate multipart;
extern crate num_cpus;
extern crate threadpool;
extern crate url;

use clap::{App, Arg};
use hyper::method::Method;
use hyper::client::request::Request;
use hyper::client::response::Response;
use hyper::status::StatusCode;
use hyper::Url;
use multipart::client::Multipart;
use std::io::{stderr, Write, Read};
use std::path::PathBuf;
use std::sync::mpsc::{channel, Sender};
use threadpool::ThreadPool;

trait UnwrapOrSend<T, E> {
        fn unwrap_or_send(self, sender: &Sender<Result<(), ()>>) -> T;
}

impl<T, E> UnwrapOrSend<T, E> for Result<T, E> {
    fn unwrap_or_send(self, sender: &Sender<Result<(),()>>) -> T {
        match self {
            Ok(val) => val,
            Err(_) => {
                let _ = sender.send(Err(()));
                panic!();
            },
        }
    }
}

fn is_number(n: String) -> Result<(), String> {
    match n.parse::<usize>() {
        Ok(val) => {
            if val == 0 {
                Err(String::from("CONCURRENT UPLOADS cannot be zero"))
            } else {
                Ok(())
            }
        },
        Err(_) => Err(String::from("CONCURRENT UPLOADS must be an integer")),
    }
}

fn upload_files(files: Vec<PathBuf>, concurrent: usize, verbose: bool) {
    if verbose {
        println!("Concurrent uploads: {}", concurrent);
    }

    let pool = ThreadPool::new(concurrent);
    let (tx, rx) = channel::<Result<(), ()>>();

    for file in &files {
        let file = file.clone();
        let tx = tx.clone();
        pool.execute(move|| {
            if verbose {
                let _ = stderr()
                    .write(format!("Uploading {}\n", &file.display()).as_bytes());
            }

            let request =
                Request::new(Method::Post,
                Url::parse("https://api.teknik.io/v1/Upload").unwrap_or_send(&tx))
                .unwrap_or_send(&tx);

            let mut multipart = Multipart::from_request(request).unwrap_or_send(&tx);

            let _ = multipart.write_file("file", &file).unwrap_or_send(&tx);
            let mut response: Response = multipart.send().unwrap_or_send(&tx);

            let mut reply = String::new();
            let _ = response.read_to_string(&mut reply).unwrap_or_send(&tx);

            if let StatusCode::Ok = response.status {
                println!("{}\n", reply);
            }

            let _ = &tx.send(Ok(()));
        });
    }

    let mut counter: usize = 0;
    for _ in rx {
        counter += 1;
        if counter == files.len() {
            break
        }
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
             .validator(is_number)
             .value_name("CONCURRENT UPLOADS")
             .help("Sets the number of concurrent uploads. The default is equal to the number of CPU processors of the current machine")
             .takes_value(true))
        .get_matches();

    let files_vec = matches.values_of_os("file")
        .unwrap()
        .map(PathBuf::from)
        .collect::<Vec<_>>();

    let concurrent_uploads: usize = matches.value_of("concurrent")
        .and_then(|s| s.parse().ok())
        .unwrap_or(num_cpus::get());

    let verbose = matches.is_present("verbose");

    upload_files(files_vec, concurrent_uploads, verbose);
}
