use reqwest::{Client, Proxy, Response};
use reqwest::header::ContentType;
use scraper::{Html, Selector};

use super::*;
use colored::*;

#[derive(Debug, Clone)]
pub struct SiteList {
  pub success_urls: Vec<String>,
  pub failed_urls: Vec<String>,
  pub ignored: u32
}

impl SiteList {
  fn new() -> SiteList {
    SiteList {
      success_urls: vec![],
      failed_urls: vec![],
      ignored: 0
    }
  }

  fn success(&mut self, url: &str) {
    info!("Added {} to Success Entry", url.green());
    self.success_urls.push(url.to_string());
  }

  fn fail(&mut self, url: &str) {
    self.failed_urls.push(url.to_string());
  }

  fn ignore(&mut self, url: &str) {
    self.ignored += 1;
  }
}

#[derive(Clone)]
pub struct Crawler {
  client: Client,
  list: SiteList
}

impl Crawler {
  pub fn new() -> Crawler {
    logger::setup();
    info!("{}", "Initializing Dark Crawler...".bold());

    let tor_proxy = Proxy::http("http://localhost:8123").unwrap();

    let client = Client::builder()
      .proxy(tor_proxy)
      .build()
      .unwrap();

    let list = SiteList::new();

    Crawler {
      client,
      list
    }
  }

  fn parse_url(&self, url: String) -> Result<String, errors::ErrorKind> {
    if !url.contains(".onion") {
      return Err(IsClearnet);
    }

    if self.list.success_urls.contains(&url) {
      return Err(AlreadyCrawled);
    }

    if self.list.failed_urls.contains(&url) {
      return Err(PreviouslyFailed);
    }

    // TODO: Use a more precise check for relative urls
    if url.starts_with("..") {
      return Err(IsRelative);
    }

    // FIXME: Relative URLs does not start with http!
    if !url.starts_with("http") {
      return Err(NonHTTP);
    }

    Ok(url)
  }

  pub fn crawl(&mut self, url: &str) {
    // Perform the URL sanity check before proceeding
    if let Err(reason) = self.parse_url(url.to_string()) {
      match reason {
        AlreadyCrawled => (),
        IsClearnet => (),
        _ => {
          warn!("Ignored {} because {}", url.dimmed(), reason.to_string().yellow());
        }
      }

      self.list.ignore(url);

      return
    }

    let url = String::from(url);

    // Spawn a new thread to handle
    crossbeam::scope(|scope| {
      scope.spawn(|| {
        self.stats();
        info!("Fetching Resource at {}", url.cyan().bold().underline());

        // Fetch the resource via HTTP GET
        match self.client.get(&url).send() {
          Ok(res) => {
            if res.status().is_success() {
              info!("Retrieved {}. Parsing...", url);

              self.parse(&url, res);
            }
          },
          Err(err) => {
            error!("Network Error: {}", err.to_string().red());
            self.list.fail(&url);
          }
        }
      });
    });
  }

  fn stats(&self) {
    let list = self.list.clone();

    let oks = list.success_urls.len().to_string().bold();
    let fails = list.failed_urls.len().to_string().bold();
    let ignores = list.ignored.to_string().bold();

    let ok_text = format!("{} SUCCESSES", oks).green();
    let fail_text = format!("{} FAILURES", fails).red();
    let ignore_text = format!("{} IGNORED", ignores).bright_black();

    info!("--- {}, {}, {} --- ", ok_text, fail_text, ignore_text);
  }

  // TODO: Check the Content-Type Header Before Parsing!
  fn parse(&mut self, url: &str, mut res: Response) {
    match res.text() {
      Ok(body) => {
        // Append the URL to the success list
        self.list.success(url);

        // Write the result to file
        self.write_file(url, &body);

        // If it is a HTML File, parse them.
        self.parse_html(&body);
      },
      Err(err) => {
        error!("{} is not a text file. ({})", url.red(), err);
        self.list.fail(url);
      }
    }
  }

  // TODO: Write file to disk.
  fn write_file(&self, url: &str, content: &str) {

  }

  // Retrieve the URLs and crawl them
  fn parse_html(&mut self, body: &str) {
    let document = Html::parse_document(&body);
    let links = Selector::parse("a").unwrap();

    for link in document.select(&links) {
      let text: Vec<_> = link.text().collect();

      if let Some(url) = link.value().attr("href") {
        debug!("Link Found in <a>: {} ({:?})", &url.blue(), text);

        self.crawl(&url);
      } else {
        debug!("<a> does not contain href. Skipping...");
      }
    }
  }
}
