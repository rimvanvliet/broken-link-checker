use std::collections::HashSet;
use std::io;
use std::io::Write;
use std::process;
use std::time::Instant;

use clap::{arg, ArgMatches, command};
use regex::Regex;
use reqwest::{Client, Response, StatusCode};
use select::document::Document;
use select::predicate::Name;

// TODO improve error handling

struct Flags {
    debug: bool,
    progress: bool,
    timer: bool
}

struct State<'a> {
    to_be_checked_pages: &'a mut HashSet<String>,
    checked_pages: &'a mut HashSet<String>,
    checked_links: &'a mut HashSet<String>,
    // the check_result contains the result of an url check; Option<String>: None if ok, the link + error if not ok
    check_results: &'a mut HashSet<Option<String>>
}

#[tokio::main]
async fn main() {
    let start = Instant::now();

    // prepare cli arg & flags
    let matches = get_matches();
    let base_url = matches.get_one::<String>("url").unwrap();
    check_base_url(base_url);

    let flags = Flags {
        debug: matches.get_flag("debug"),
        progress: matches.get_flag("progress"),
        timer: matches.get_flag("timer")
    };

    //create the reqwest async client
    let client = Client::new();

    // initialise the state; a page is an internal link, a link is an external link
    let mut state = State {
        to_be_checked_pages: &mut HashSet::new(),
        checked_pages: &mut HashSet::new(),
        checked_links: &mut HashSet::new(),
        check_results: &mut HashSet::new()
    };

    // initialise the pages that must still be checked with the base_url
    state.to_be_checked_pages.insert(base_url.to_string());

    check_pages(base_url, &flags, &client, &mut state).await;
    // print the results
    let success = summarize_results(start, &flags.timer, state);

    // exit <> 0 if bad_urls exists
    if success {
        process::exit(0);
    } else {
        process::exit(-1);
    }
}

fn get_matches() -> ArgMatches {
    command!()
        .arg_required_else_help(true)
        .arg(arg!([url] "Required url to operate on, including the protocol (so http or https)."))
        .arg(arg!(-d --debug "Turn debugging information on.")
            .required(false))
        .arg(arg!(-p --progress "Show a progress on-liner.")
            .required(false))
        .arg(arg!(-t --timer "Time execution.")
            .required(false))
        .get_matches()
}

fn check_base_url(base_url: &String) {
    let regex = Regex::new(r"^https?://[0-9A-Za-z.:]+$").unwrap();
    if !(regex.is_match(base_url)) {
        println!("{base_url} is not a valid url.");
        process::exit(1);
    }
}

async fn check_pages(base_url: &String, flags: &Flags, client: &Client, state: &mut State<'_>) {
    if flags.progress { print_progress(state); }

    while !state.to_be_checked_pages.is_empty() {
        check_page(base_url, flags, client, state).await
    }
}

async fn check_page(base_url: &String, args: &Flags, client: &Client, state: &mut State<'_>) {

    // pop a (random) page to be checked, remove it from the pages to be checked
    let page_being_checked = state.to_be_checked_pages.iter().next().unwrap().clone();
    state.to_be_checked_pages.remove(&page_being_checked);

    if args.debug { log_start_checking(state.to_be_checked_pages, &page_being_checked); }

    let hrefs = crawl(client, &page_being_checked).await;
    if args.debug { log_new_items(&hrefs, "hrefs") }

    // determine the pages we did not yet see
    let new_pages = get_new_pages(base_url, state, &page_being_checked, &hrefs);

    if args.debug { log_new_items(&new_pages, "new_pages") }

    // determine the links we did not yet see
    let new_links = get_new_links(base_url, state, &page_being_checked, hrefs);

    if args.debug { log_new_items(&new_links, "new_links") }

    // concatenate new_pages and new_urls to check them in a batch
    let new_urls = [&Vec::from_iter(new_pages.clone())[..], &Vec::from_iter(new_links.clone())[..]].concat();

    if args.debug { log_start_checking_links(state, &page_being_checked); }

    for check_result in check_urls(client, &page_being_checked, new_urls, args.debug).await {
        state.check_results.insert(check_result);
    }

    // insert the new_pages into the to_be_checked_pages
    insert_newpages_into_tobecheckedpages(state.to_be_checked_pages, new_pages);

    // and insert the new_links into the checked_links
    insert_newlinks_into_checkedlinks(state.checked_links, new_links);

    // finally add the page_being_checked to the checked_pages
    state.checked_pages.insert(page_being_checked.clone());

    if args.debug { println!("=============== end checking {}", page_being_checked); }
    if args.progress { print_progress(state); }
}

fn summarize_results(start: Instant, timer: &bool, state: State) -> bool {
    println!("\n--> de gevonden pagina's van de website zijn ({}): ", state.checked_pages.len());
    for checked_page in state.checked_pages.clone().into_iter() {
        println!("{checked_page}");
    }
    println!("--> de gecheckte externe links zijn ({}): ", state.checked_links.len());
    for checked_link in state.checked_links.clone().into_iter() {
        println!("{checked_link}");
    }
    let bad_results = state.check_results
        .clone()
        .into_iter()
        .flatten().collect::<Vec<String>>();
    if bad_results.is_empty() {
        println!("--> er zijn GEEN gebroken urls.");
    } else {
        println!("--> LET OP: ER ZIJN GEBROKEN URLS ({}):", bad_results.len());
        for bad_result in &bad_results {
            println!("{bad_result}");
        }
    }
    if *timer { println!("Timer: {} seconden.", start.elapsed().as_secs()); }
    bad_results.is_empty()
}

fn insert_newlinks_into_checkedlinks(checked_links: &mut HashSet<String>, new_links: HashSet<String>) {
    new_links
        .into_iter()
        .for_each(|item| {
            checked_links.insert(item);
        });
}

fn insert_newpages_into_tobecheckedpages(to_be_checked_pages: &mut HashSet<String>, new_pages: HashSet<String>) {
    new_pages
        .into_iter()
        .for_each(|item| {
            to_be_checked_pages.insert(item);
        });
}

fn log_start_checking_links(state: &mut State, page_being_checked: &String) {
    println!("=============== start checking links found in {}", page_being_checked);
    state.to_be_checked_pages.clone().into_iter().for_each(|s| println!("{s}, "));
}

fn get_new_links(base_url: &String, state: &mut State, page_being_checked: &str, hrefs: HashSet<String>) -> HashSet<String> {
    hrefs
        .into_iter()
        .map(|href| format_url(&href, base_url, page_being_checked))
        .filter(|href| href.starts_with("http")
            && !href.starts_with(base_url)
            && !state.checked_links.contains(href))
        .collect::<HashSet<String>>()
}

fn get_new_pages(base_url: &String, state: &mut State, page_being_checked: &String, hrefs: &HashSet<String>) -> HashSet<String> {
    hrefs
        .clone()
        .into_iter()
        .map(|href| format_url(&href, base_url, page_being_checked))
        .filter(|href| (href.starts_with(base_url) || !href.contains(':'))
            && href != page_being_checked
            && !href.is_empty()
            && !state.checked_pages.contains(href)
            && !state.to_be_checked_pages.contains(href))
        .collect::<HashSet<String>>()
}

fn log_start_checking(to_be_checked_pages: &mut HashSet<String>, page_being_checked: &String) {
    println!("\n=============== start checking {}, remaining {}", page_being_checked, to_be_checked_pages.len());
    to_be_checked_pages.clone().into_iter().for_each(|s| println!("{s}, "));
}

fn print_progress(state: &mut State) {
    print!("\rInternal pages checked: {}, Pages to go: {}, External links checked: {}                         ", state.checked_pages.len(), state.to_be_checked_pages.len(), state.checked_links.len());
    io::stdout().flush().unwrap();
}

// strip a trailing '/' and/or add the base_url or page_being_checked to the url
fn format_url(href: &str, base_url: &str, page_being_checked: &str) -> String {
    let mut tmp_href = href.to_string();
    let mut tmp_page_being_checked = page_being_checked.to_string();
    if tmp_href.ends_with('/') { tmp_href.pop(); }
    if tmp_page_being_checked.ends_with('/') { tmp_page_being_checked.pop(); }
    if tmp_href.starts_with('/') { format!("{}{}", base_url, tmp_href) } // an absolute local URL
    else if !tmp_href.contains(':') { format!("{:?}/{}", tmp_page_being_checked, tmp_href) } // a relative URL
    else { tmp_href }
}

// fetch all href's from the page, so both pages & links
async fn crawl(client: &Client, url: &str) -> HashSet<String> {
    let mut links = HashSet::new();
    let body = client.get(url).send().await.unwrap().text().await.unwrap();
    let document = Document::from(body.as_str());
    for node in document.find(Name("a")) {
        let link = node.attr("href").unwrap_or("").to_string();
        links.insert(link);
    }
    links
}

// checks Vec<url> (pages or links) for HTTP status code between 200 and 299
// returns None if ok, Some(link+error) if not ok
async fn check_urls(client: &Client, page_being_checked: &str, urls: Vec<String>, debug: bool) -> HashSet<Option<String>> {
    let mut check_results = HashSet::new();

    // spawn tasks to concurrently & async check the urls
    let tasks = urls.into_iter().map(move |url| {
        fetch_url(client, url)
    });
    let responses = futures::future::join_all(tasks).await;

    for response in responses {
        match response {
            Ok(res) => {
                if res.status() >= StatusCode::from_u16(200).unwrap()
                    && res.status() < StatusCode::from_u16(300).unwrap() {
                    if debug { println!("{}: success {:?}", res.url(), res.status()); }
                    check_results.insert(None);
                } else {
                    if debug { println!("!!!!! ERROR {}: {:?}", res.url(), res.status()); }
                    check_results.insert(Some(format!("{} on {} gave status {:?}", res.url(), page_being_checked, res.status())));
                }
            }
            Err(err) => {
                if debug { println!("!!!!! ERROR {:?}", err); }
                check_results.insert(Some(format!("error {:?}", err)));
            }
        }
    }
    check_results
}

// fetches a single url
async fn fetch_url(client: &Client, url: String) -> Result<Response, Box<dyn std::error::Error>> {
    Ok(client
        .get(url)
        .header("User-Agent", "Mozilla/5.0 (Macintosh; Intel Mac OS X 10.15; rv:108.0) Gecko/20100101 Firefox/108.0")
        .header("Accept", "text/html,application/xhtml+xml,application/xml;q=0.9,image/avif,image/webp,*/*;q=0.8")
        .header("Accept-Language", "nl,en-US;q=0.7,en;q=0.3")
        .header("Accept-Encoding", "gzip, deflate, br")
        .header("DNT", "1")
        .header("Connection", "keep-alive")
        .header("Upgrade-Insecure-Requests", "1")
        .header("Sec-Fetch-Dest", "document")
        .header("Sec-Fetch-Mode", "navigate")
        .header("Sec-Fetch-Site", "none")
        .header("Sec-Fetch-User", "?1")
        .header("Pragma", "no-cache")
        .header("Cache-Control", "no-cache")
        .send()
        .await?)
}

fn log_new_items(hash_set: &HashSet<String>, item_name: &str) {
    if !hash_set.is_empty() {
        println!("=============== {} ({}); ", item_name, hash_set.len());
        hash_set.clone().into_iter().for_each(|s| println!("{s},"));
        println!("=============== end of {}", item_name);
    } else {
        println!("=============== no {}", item_name);
    }
}

