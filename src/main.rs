use std::collections::HashSet;
use reqwest::blocking::Client;
use reqwest::StatusCode;
use select::document::Document;
use select::predicate::Name;

fn main() {
    let debug = false;
    let base_url = "http://stevensbikeservice.nl";

    let client = Client::new();

    // initialise the progress; a page is an internal link, a link is an external link
    let mut checked_pages = HashSet::new();
    let mut checked_links = HashSet::new();
    // the check_result is Option<String>: None if ok, the link + error if not ok
    let mut check_results = HashSet::new();
    // initialise the pages that must still be checked with the root of the website
    let mut to_be_checked_pages = HashSet::new();
    to_be_checked_pages.insert("/".to_string());

    while to_be_checked_pages.len() > 0 {
        // pop a random page to be checked
        let page_being_checked = to_be_checked_pages.iter().next().unwrap().clone();
        to_be_checked_pages.remove(&page_being_checked);

        if debug {
            println!("=============== Checking {page_being_checked} of {} ====================", to_be_checked_pages.len());
            to_be_checked_pages.clone().into_iter().for_each(|s| print!("{s}, "));
            println!();
        }

        let hrefs = crawl(&client, &format!("{base_url}{page_being_checked}"));

        // determine the pages we did not yet see
        let new_pages = hrefs
            .clone()
            .into_iter()
            .map(|href| strip_url(&href, &base_url))
            .filter(|href| (href.starts_with("/") || href.starts_with(base_url))
                && href != page_being_checked
                && !checked_pages.contains(href)
                && !to_be_checked_pages.contains(href))
            .collect::<HashSet<String>>();

        if debug {
            print!("new pages ({}); ", new_pages.len());
            new_pages.clone().into_iter().for_each(|s| print!("{s}, "));
            println!();
        }

        // check all new pages
        new_pages
            .clone()
            .into_iter()
            .for_each(|new_page| {
                check_results.insert(check_url(&client, &format!("{base_url}{new_page}"), debug));
            });

        // and insert them into the to_be_checked_pages
        new_pages
            .into_iter()
            .for_each(|item| {
                to_be_checked_pages.insert(item);
            });

        // determine the links we did not yet see
        let new_links = hrefs
            .into_iter()
            .map(|href| strip_url(&href, &base_url))
            .filter(|href| href.starts_with("http") && !checked_links.contains(href))
            .collect::<HashSet<String>>();

        // check them
        new_links
            .clone()
            .into_iter()
            .for_each(|new_link| {
                check_results.insert(check_url(&client, &new_link, debug));
            });

        // and add them to the checked_links
        new_links
            .into_iter()
            .for_each(|item| {
                checked_links.insert(item);
            });

        // finally add the page_being_checked to the checked_pages
        checked_pages.insert(page_being_checked);
    }

    // print the results
    println!("--> de gevonden interne links zijn ({}): ", checked_pages.len());
    for int_link in checked_pages {
        println!("{int_link}");
    }
    println!("--> de gecheckte externe links zijn ({}): ", checked_links.len());
    for checked_url in checked_links {
        println!("{checked_url}");
    }
    let bad_urls = check_results
        .into_iter()
        .flatten().collect::<Vec<_>>();
    let nr_bad_urls = bad_urls.clone().len();
    if nr_bad_urls == 0 {
        println!("--> er zijn GEEN gebroken urls.");
    } else {
        println!("--> LET OP: ER ZIJN GEBROKEN URLS ({}):", bad_urls.clone().len());
        // println!("--> bad urls ({})", &bad_urls.cloned().collect::<Vec<_>>().len());
        for bad_url in bad_urls {
            println!("{bad_url}");
        }
    }

    // strip a trailing '/' and/or the base_url from the url
    fn strip_url(s: &str, prefix: &str) -> String {
        let mut chars = s.chars();
        if s.ends_with("/") {
            chars.next_back();
        }
        if s.starts_with(prefix) {
            for _i in 0..prefix.len() {
                chars.next();
            }
        }
        chars.as_str().to_string()
    }

    // fetch all href's from the page, so both pages & links
    fn crawl(client: &Client, url: &str) -> HashSet<String> {
        let mut links = HashSet::new();
        let body = client.get(url).send().unwrap().text().unwrap();
        let document = Document::from(body.as_str());
        for node in document.find(Name("a")) {
            let link = node.attr("href").unwrap_or("");
            links.insert(link.to_string());
        }
        links
    }

    // check url (page or link) for HTTP status code between 200 and 399
    // return None if ok, Some(link+error) if not ok
    // TODO accept Vec of urls and check async in parallel
    fn check_url(client: &Client, url: &str, debug: bool) -> Option<String> {
        let response =
            client
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
                .send();
        match response {
            Ok(res) => {
                if res.status() >= StatusCode::from_u16(200).unwrap()
                    && res.status() < StatusCode::from_u16(400).unwrap() {
                    if debug { println!("{}: success {:?}", url, res.status()); }
                    None
                } else {
                    println!("!!!!! ERROR {}: {:?}", url, res.status());
                    Some(format!("{link} gave status {:?}", res.status()))
                }
            }
            Err(err) => {
                println!("!!!!! ERROR: {}: {}", url, err);
                Some(format!("{link} gave error {:?}", err))
            }
        }
    }
}

