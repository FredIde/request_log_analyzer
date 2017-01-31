use std::io;
use std::io::prelude::*;
use std::net::TcpStream;
use std::fs::File;
use std::env;

#[macro_use]
extern crate log;
extern crate env_logger;

extern crate chrono;
use chrono::*;

extern crate stats;
#[macro_use]
extern crate clap;

mod args;
mod http_status;
mod log_parser;
use log_parser::parse_line;
use log_parser::log_events::*;
mod request_response_matcher;
use request_response_matcher::*;
mod filter;
mod analyzer;
mod render;
use render::*;

fn main() {
    env_logger::init().unwrap();

    let args = args::parse_args(env::args()).unwrap();

    let input: Box<io::Read> = match args.filename.as_ref() {
        "-" => Box::new(io::stdin()),
        _ => Box::new(File::open(&args.filename).unwrap()),
    };

    let timings = extract_timings(input, &args.conditions);

    let result = analyzer::analyze(&timings);

    let mut stream;
    let mut renderer: Box<Renderer>;

    renderer = match args.graphite_server {
        Some(graphite_server) => {
            stream = TcpStream::connect((graphite_server.as_ref(), args.graphite_port.unwrap()))
                .expect("Could not connect to the Graphite server");

            Box::new(GraphiteRenderer::new(UTC::now(), args.graphite_prefix, &mut stream))
        }
        None => Box::new(TerminalRenderer::new()),
    };

    match result {
        Some(result) => {
            renderer.render(result);
        }
        None => warn!("No matching log lines in file."),
    }
}

fn extract_timings(input: Box<io::Read>, conditions: &filter::FilterConditions) -> Vec<i64> {
    let reader = io::BufReader::new(input);
    let lines = reader.lines();

    let mut requests: Vec<Request> = Vec::new();
    let mut responses: Vec<Response> = Vec::new();
    let mut timings: Vec<i64> = Vec::new();

    for line in lines {
        let line_value = &line.unwrap();

        let parsed_line = parse_line(line_value);

        match parsed_line {
            Ok(event) => {
                match event {
                    LogEvent::Request(request) => requests.push(request),
                    LogEvent::Response(response) => responses.push(response),
                }

                let pairs = extract_matching_request_response_pairs(&mut requests, &mut responses);

                let mut new_timings: Vec<i64> = pairs.iter()
                    .filter(|pair| filter::matches_filter(&pair, conditions))
                    .map(|pair| pair.response.response_time.num_milliseconds())
                    .collect();

                timings.append(&mut new_timings);
            }
            Err(err) => warn!("{}", err),
        }
    }
    timings
}

#[test]
fn test_extract_timings() {
    let conditions = filter::FilterConditions {
        include_terms: None,
        exclude_terms: None,
        latest_time: None,
    };

    let timings = extract_timings(Box::new(File::open("src/test/simple-1.log").unwrap()),
                                  &conditions);

    assert_eq!(timings, vec![7, 10]);
}

#[test]
fn test_extract_timings_ignore_broken_lines() {
    let conditions = filter::FilterConditions {
        include_terms: None,
        exclude_terms: None,
        latest_time: None,
    };

    let timings = extract_timings(Box::new(File::open("src/test/broken.log").unwrap()),
                                  &conditions);

    assert_eq!(timings, vec![7]);
}
