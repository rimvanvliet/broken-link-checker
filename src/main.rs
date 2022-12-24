use std::collections::HashSet;
use reqwest::blocking::Client;
use reqwest::StatusCode;
use select::document::Document;
use select::predicate::Name;

fn main() {
    let client = Client::new();
    let mut visited_urls = std::collections::HashSet::new();
    let mut checked_urls = std::collections::HashSet::new();
    let mut bad_urls = std::collections::HashSet::new();

    // Crawl the website and get all of the links
    let base_url = "http://vliet.io";
    let links = crawl(&client, base_url);

    // Check each link to see if it is valid
    for link in links {
        if link.starts_with('/') {
            let internal_link = format!("{base_url}{link}");
            bad_urls.insert(check_link(&client, &internal_link));
            let link_copy = format!("{link}");
            println!("checking {link_copy}");
            visited_urls.insert(link);
            let sublinks = crawl(&client, &internal_link);
            for sublink in sublinks {
                if sublink.starts_with("/") {
                    if !visited_urls.contains(&sublink) {
                        println!("     >>> {sublink} is nieuw in {link_copy}");
                        bad_urls.insert(check_link(&client, &format!("{base_url}{sublink}")));
                        visited_urls.insert(sublink);
                    }
                } else if sublink.starts_with("http") {
                    if !checked_urls.contains(&sublink) {
                        checked_urls.insert(sublink.to_string());
                        bad_urls.insert(check_link(&client, &sublink));
                    }
                }
            }
        } else if link.starts_with("http") {
            if !checked_urls.contains(&link) {
                println!("     >>> {link}  checken.");
                checked_urls.insert(link.to_string());
                bad_urls.insert(check_link(&client, &link));
            }
        }
    }
    println!("--> de gevonden interne links zijn ({}): ", visited_urls.len());
    for int_link in visited_urls {
        println!("{int_link}");
    }

    println!("--> de gecheckte externe links zijn ({}): ", checked_urls.len());
    for checked_url in checked_urls {
        println!("{checked_url}");
    }

    println!("--> bad urls ({})",
             bad_urls
                 .into_iter()
                 .flatten()
                 .collect::<Vec<String>>()
                 .len());

    fn crawl(client: &reqwest::blocking::Client, url: &str) -> HashSet<String> {
        let mut links = HashSet::new();
        let body = client.get(url).send().unwrap().text().unwrap();
        let document = Document::from(body.as_str());
        for node in document.find(Name("a")) {
            let link = node.attr("href").unwrap_or("");
            links.insert(link.to_string());
        }
        links
    }

    fn check_link(client: &reqwest::blocking::Client, link: &str) -> Option<String> {
        let response =
            client
                .get(link)
                .header("User-Agent", "Ruud's HTTP health check")
                .send();
        match response {
            Ok(res) => {
                if res.status() >= StatusCode::from_u16(200).unwrap()
                    && res.status() < StatusCode::from_u16(400).unwrap() {
                    println!("{}: success {:?}", link, res.status());
                    None
                } else {
                    println!("!!!!! ERROR {}: {:?}", link, res.status());
                    Some(format!("{link} gave status {:?}", res.status()))
                }
            }
            Err(err) => {
                println!("!!!!! ERROR: {}: {}", link, err);
                Some(format!("{link} gave error {:?}", err))
            }
        }
    }
}

