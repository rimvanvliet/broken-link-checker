mod matches;

use matches::{Flags, get_arch_matches};

use std::collections::HashSet;
use std::io;
use std::io::Write;
use std::process;
use std::time::Instant;

use reqwest::{Client, Response, StatusCode};
use select::document::Document;
use select::predicate::Name;
use url::{Url};

use chrono::Local;
use env_logger::Builder;
use log::LevelFilter;

// TODO improve error handling

struct State<'a> {
    to_be_checked_pages: &'a mut HashSet<String>,
    checked_pages: &'a mut HashSet<String>,
    checked_links: &'a mut HashSet<String>,
    // the check_result contains the result of an url check; Option<String>: None if ok, the link + error if not ok
    check_results: &'a mut HashSet<Option<String>>,
}

#[tokio::main]
async fn main() {

    Builder::new()
        .format(|buf, record| {
            writeln!(buf,
                     "{} [{}] - {}",
                     Local::now().format("%Y-%m-%dT%H:%M:%S"),
                     record.level(),
                     record.args()
            )
        })
        .filter(None, LevelFilter::Info)
        .init();

    let start = Instant::now();

    // prepare cli args & flags
    let (base_url, flags) = get_arch_matches();
    // initialise the state; a page is an internal link, a link is an external link
    let mut state = State {
        to_be_checked_pages: &mut HashSet::new(),
        checked_pages: &mut HashSet::new(),
        checked_links: &mut HashSet::new(),
        check_results: &mut HashSet::new(),
    };

    // create the reqwest async client with sensible timeouts
    let client = Client::builder()
        .connect_timeout(std::time::Duration::from_secs(10))
        // default overall request timeout (can be overridden per-request)
        .timeout(std::time::Duration::from_secs(60))
        .build()
        .expect("failed to build reqwest client");

    // initialise the pages that must still be checked with the base_url
    state.to_be_checked_pages.insert(base_url.to_string());

    check_pages(&base_url, &flags, &client, &mut state).await;
    // print the results
    let success = summarize_results(start, &flags.timer, state);

    // exit <> 0 if bad_urls exists
    if success {
        process::exit(0);
    } else {
        process::exit(-1);
    }
}

async fn check_pages(base_url: &str, flags: &Flags, client: &Client, state: &mut State<'_>) {
    if flags.progress { print_progress(state); }

    while !state.to_be_checked_pages.is_empty() {
        check_page(base_url, flags, client, state).await
    }
}

async fn check_page(base_url: &str, args: &Flags, client: &Client, state: &mut State<'_>) {

    // pop a (random) page to be checked, remove it from the pages to be checked
    let page_being_checked = state.to_be_checked_pages.iter().next().unwrap().clone();
    state.to_be_checked_pages.remove(&page_being_checked);
    if args.debug { println!("\n=============== start checking {}, remaining {}", page_being_checked, state.to_be_checked_pages.len()); }

    // get all hrefs in the page being checked
    let hrefs = crawl(client, &page_being_checked).await;
    if args.debug { log_new_items(&hrefs, "hrefs") }

    // determine the pages we did not yet see
    let new_pages = get_new_pages(base_url, state, &page_being_checked, &hrefs);
    if args.debug { log_new_items(&new_pages, "new_pages") }

    // determine the links we did not yet see
    let new_links = get_new_links(base_url, state, &page_being_checked, hrefs);
    if args.debug { log_new_items(&new_links, "new_links") }

    // concatenate new_pages and new_links to check them in a batch
    let new_urls = [&Vec::from_iter(new_pages.clone())[..], &Vec::from_iter(new_links.clone())[..]].concat();
    if args.debug { println!("=============== start checking links found in {}", page_being_checked); }

    for check_result in check_urls(client, &page_being_checked, new_urls, args.debug, args.fetched_urls).await {
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
async fn check_urls(client: &Client, page_being_checked: &str, urls: Vec<String>, debug: bool, fetched_urls: bool) -> HashSet<Option<String>> {
    let mut check_results = HashSet::new();

    // Limit concurrent requests to avoid overwhelming servers like Airbnb
    let semaphore = std::sync::Arc::new(tokio::sync::Semaphore::new(5)); // max 5 concurrent requests
    
    // spawn tasks to concurrently & async check the urls  
    let tasks = urls.into_iter().map(|url| {
        let client = client.clone();
        let semaphore = semaphore.clone();
        async move {
            let _permit = semaphore.acquire().await.unwrap();
            if fetched_urls { log::info!("fetching {}", url); }
            
            // Add small delay between requests to be more respectful
            tokio::time::sleep(std::time::Duration::from_millis(200 + (rand::random::<u64>() % 300))).await;
            
            fetch_url(&client, url).await
        }
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

// fetches a single url with retry logic
async fn fetch_url(client: &Client, url: String) -> Result<Response, Box<dyn std::error::Error>> {
    let max_retries = 5;
    let mut retry_count = 0;

    // allow a longer timeout for hosts that are known to be slow/unreliable
    let per_request_timeout = if url.contains("stichtingmtbsalland.nl") {
        std::time::Duration::from_secs(120)
    } else {
        std::time::Duration::from_secs(30)
    };

    loop {
        let result = client
            .get(&url)
            .timeout(per_request_timeout)
            .header("User-Agent", "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/119.0.0.0 Safari/537.36")
            .header("Accept", "text/html,application/xhtml+xml,application/xml;q=0.9,image/avif,image/webp,image/apng,*/*;q=0.8,application/signed-exchange;v=b3;q=0.7")
            .header("Accept-Language", "nl-NL,nl;q=0.9,en;q=0.8")
            .header("Accept-Encoding", "gzip, deflate, br")
            .header("DNT", "1")
            .header("Connection", "keep-alive")
            .header("Upgrade-Insecure-Requests", "1")
            .header("Sec-Fetch-Dest", "document")
            .header("Sec-Fetch-Mode", "navigate")
            .header("Sec-Fetch-Site", "none")
            .header("Sec-Fetch-User", "?1")
            .header("Sec-Ch-Ua", "\"Google Chrome\";v=\"119\", \"Chromium\";v=\"119\", \"Not?A_Brand\";v=\"24\"")
            .header("Sec-Ch-Ua-Mobile", "?0")
            .header("Sec-Ch-Ua-Platform", "\"macOS\"")
            .send()
            .await;
        
        match result {
            Ok(response) => return Ok(response),
            Err(err) => {
                retry_count += 1;
                if retry_count >= max_retries {
                    return Err(Box::new(err));
                }

                log::warn!("Request failed for {}, attempt {}/{}. Error: {}", url, retry_count, max_retries, err);

                // Exponential backoff with some jitter (increase base delay)
                let delay_ms = (2000 * (2_u64.pow(retry_count as u32 - 1))) + (rand::random::<u64>() % 2000);
                tokio::time::sleep(std::time::Duration::from_millis(delay_ms)).await;
            }
        }
    }
}

fn get_new_pages(base_url: &str, state: &mut State, page_being_checked: &String, hrefs: &HashSet<String>) -> HashSet<String> {
    let bare_base_url = format_url("/", base_url);
    hrefs
        .into_iter()
        .map(|href| format_url(&href, page_being_checked))
        .filter(|href| (href.starts_with(&bare_base_url) || !href.contains(':'))
            && href != page_being_checked
            && !href.is_empty()
            && !state.checked_pages.contains(href)
            && !state.to_be_checked_pages.contains(href))
        .collect::<HashSet<String>>()
}

fn get_new_links(base_url: &str, state: &mut State, page_being_checked: &str, hrefs: HashSet<String>) -> HashSet<String> {
    let bare_base_url = format_url("/", base_url);
    hrefs
        .into_iter()
        .map(|href| format_url(&href, page_being_checked))
        .filter(|href| href.starts_with("http")
            && !href.starts_with(&bare_base_url)
            && !state.checked_links.contains(href))
        .collect::<HashSet<String>>()
}

fn insert_newpages_into_tobecheckedpages(to_be_checked_pages: &mut HashSet<String>, new_pages: HashSet<String>) {
    new_pages
        .into_iter()
        .for_each(|item| {
            to_be_checked_pages.insert(item);
        });
}

fn insert_newlinks_into_checkedlinks(checked_links: &mut HashSet<String>, new_links: HashSet<String>) {
    new_links
        .into_iter()
        .for_each(|item| {
            checked_links.insert(item);
        });
}

fn log_new_items(hash_set: &HashSet<String>, item_name: &str) {
    if !hash_set.is_empty() {
        log::info!("=============== {} ({}); ", item_name, hash_set.len());
        hash_set.clone().into_iter().for_each(|s| println!("{s},"));
        println!("=============== end of {}", item_name);
    } else {
        println!("=============== no {}", item_name);
    }
}

fn print_progress(state: &mut State) {
    print!("\rInternal pages checked: {}, Pages to go: {}, External links checked: {}                         ", state.checked_pages.len(), state.to_be_checked_pages.len(), state.checked_links.len());
    io::stdout().flush().unwrap();
}

fn format_url(href: &str,  page_being_checked: &str) -> String {
    let new_base_url = Url::parse(page_being_checked);
    let combined_url = new_base_url.expect("dat ging fout").join(href);

    match combined_url {
        Ok(url) => strip_trailing_slash(url),
        Err(e) => e.to_string()
    }
}

fn strip_trailing_slash(url: Url) -> String {
    let mut url_as_string = url.to_string();
    if url_as_string.ends_with('/') { url_as_string.pop(); }
    url_as_string
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
