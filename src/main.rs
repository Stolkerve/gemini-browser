use actix_files as fs;
use actix_web::{get, web, App, HttpServer, Responder, Result};
use native_tls::TlsConnector as NativeTlsConnector;
use serde::{Deserialize, Serialize};
use tinytemplate::TinyTemplate;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio_native_tls::TlsConnector;
use url::Url;

const INDEX_HTML_TEMPLATE: &'static str = include_str!("./static/index.html");

#[derive(Deserialize)]
struct SearchQuery {
    pub search: Option<String>,
    pub promp: Option<usize>,
}
#[derive(Serialize)]
struct Context {
    name: String,
}

#[tokio::main]
async fn main() -> std::io::Result<()> {
    HttpServer::new(|| {
        App::new()
            .service(fs::Files::new("/assets", "./src/static/assets"))
            .service(bro)
    })
    .bind(("0.0.0.0", 3000))?
    .run()
    .await
}

#[get("/")]
async fn bro(params: web::Query<SearchQuery>) -> Result<impl Responder> {
    let mut tt = TinyTemplate::new();
    tt.set_default_formatter(&tinytemplate::format_unescaped);
    tt.add_template("template", INDEX_HTML_TEMPLATE).unwrap();
    if let Some(search) = &params.search {
        match handle_request(search).await {
            Ok(gems_html) => {
                let context = Context { name: gems_html };
                return Ok(web::Html::new(tt.render("template", &context).unwrap()));
            }
            Err(err) => {
                let context = Context { name: err.into() };
                return Ok(web::Html::new(tt.render("template", &context).unwrap()));
            }
        };
    }

    let context = Context {
        name: "".to_string(),
    };
    Ok(web::Html::new(tt.render("template", &context).unwrap()))
}

pub async fn handle_request(search: &String) -> Result<String, &str> {
    let mut search = search.clone();
    if search.len() == 0 {
        return Err("ERROR: empty search");
    }
    if !search.starts_with("gemini://") {
        search.insert_str(0, "gemini://");
    }

    let url = Url::parse(&search);
    if let Ok(mut url) = url {
        if url.port() == None {
            let _ = url.set_port(Some(1965));
        }
        url.path();

        let mut redirect_loop = 0;
        let host_str = url.host_str().unwrap();
        let mut request = format!("{}{}", host_str, url.path());
        println!("{}", request);
        while redirect_loop != 15 {
            match make_gemini_request(url.authority(), &request).await {
                Ok(res_buff) => {
                    let gemini_response = decode_response(&res_buff);
                    match gemini_response.status {
                        GeminiStatus::Input => {
                            // dialog::input_default("asd", "asd");
                            continue;
                        }
                        GeminiStatus::SensitiveInput => todo!(),
                        GeminiStatus::TemporaryRedirect | GeminiStatus::PermanentRedirect => {
                            let url = gemini_response.info;
                            match Url::parse(&url) {
                                Ok(url) => {
                                    request = format!("{}{}", url.host_str().unwrap(), url.path());
                                }
                                Err(_) => {
                                    return Err("ERROR: Invalid redirect");
                                }
                            };
                            redirect_loop += 1;
                            continue;
                        }
                        GeminiStatus::Success => {
                            let document =
                                String::from_utf8_lossy(&gemini_response.body).to_string();
                            return Ok(parse_document_to_gems(&document, host_str));
                        }
                        status => {
                            return Err(status.to_str());
                        }
                    };
                }
                Err(_) => {
                    return Err("ERROR: connection fail");
                }
            }
        }
        return Err("ERROR: Unknown ;(");
    } else {
        return Err("ERROR: Bad url");
    }
}

#[derive(Debug, Clone)]
pub enum GeminiStatus {
    Input = 10,
    SensitiveInput = 11,
    Success = 20,
    TemporaryRedirect = 30,
    PermanentRedirect = 31,
    TemporaryFailure = 40,
    ServerUnavailable = 41,
    CGIError = 42,
    ProxyError = 43,
    SlowDown = 44,
    PermanentFailure = 50,
    NotFound = 51,
    Gone = 52,
    ProxyRequestRefused = 53,
    BadRequest = 59,
    ClientCertificateRequired = 60,
    CertificateNotAuthorized = 61,
    CertificateNotValid = 62,
}

impl GeminiStatus {
    pub fn to_str(&self) -> &'static str {
        return match self {
            GeminiStatus::Input => "input",
            GeminiStatus::SensitiveInput => "sensitive input",
            GeminiStatus::Success => "success",
            GeminiStatus::TemporaryRedirect => "temporary redirect",
            GeminiStatus::PermanentRedirect => "permanent redirect",
            GeminiStatus::TemporaryFailure => "temporary failure",
            GeminiStatus::ServerUnavailable => "server unavailable",
            GeminiStatus::CGIError => "CGI error",
            GeminiStatus::ProxyError => "proxy error",
            GeminiStatus::SlowDown => "slow down",
            GeminiStatus::PermanentFailure => "permanent failure",
            GeminiStatus::NotFound => "not found",
            GeminiStatus::Gone => "gone",
            GeminiStatus::ProxyRequestRefused => "proxy request refused",
            GeminiStatus::BadRequest => "bad request",
            GeminiStatus::ClientCertificateRequired => "client certificate required",
            GeminiStatus::CertificateNotAuthorized => "certificate not authorized",
            GeminiStatus::CertificateNotValid => "certificate not valid",
        };
    }

    pub fn from_u8(v: u8) -> Option<GeminiStatus> {
        match v {
            10 => Some(GeminiStatus::Input),
            11 => Some(GeminiStatus::SensitiveInput),
            20 => Some(GeminiStatus::Success),
            30 => Some(GeminiStatus::TemporaryRedirect),
            31 => Some(GeminiStatus::PermanentRedirect),
            40 => Some(GeminiStatus::TemporaryFailure),
            41 => Some(GeminiStatus::ServerUnavailable),
            42 => Some(GeminiStatus::CGIError),
            43 => Some(GeminiStatus::ProxyError),
            44 => Some(GeminiStatus::SlowDown),
            50 => Some(GeminiStatus::PermanentFailure),
            51 => Some(GeminiStatus::NotFound),
            52 => Some(GeminiStatus::Gone),
            53 => Some(GeminiStatus::ProxyRequestRefused),
            59 => Some(GeminiStatus::BadRequest),
            60 => Some(GeminiStatus::ClientCertificateRequired),
            61 => Some(GeminiStatus::CertificateNotAuthorized),
            62 => Some(GeminiStatus::CertificateNotValid),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct GeminiResponse {
    pub status: GeminiStatus,
    pub info: String,
    pub body: Vec<u8>,
}

impl GeminiResponse {
    pub fn new(status: GeminiStatus, info: String, body: Vec<u8>) -> Self {
        Self { status, info, body }
    }
}

pub async fn make_gemini_request(
    url: &str,
    request: &str,
) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let mut connector = NativeTlsConnector::builder();
    connector.danger_accept_invalid_certs(true); // Accept self-signed certificates
    let connector = connector.build()?;
    let connector = TlsConnector::from(connector);

    // Connect to the server
    let stream = TcpStream::connect(url).await?;
    let mut tls_stream = connector.connect("geminiprotocol.net", stream).await?;

    tls_stream
        .write(format!("gemini://{}\r\n", request).as_bytes())
        .await
        .unwrap();

    let mut res_body = Vec::<u8>::with_capacity(1024 << 6);
    let mut read_buf = vec![0; 1024];
    let mut n = tls_stream.read(&mut read_buf).await?;
    while n > 0 {
        // romper si supera algun limite de memoria, 10MB?
        res_body.extend_from_slice(&read_buf[..n]);
        n = tls_stream.read(&mut read_buf).await?;
        if n == 0 {
            break;
        }
    }
    return Ok(res_body);
}

pub fn decode_response(res_buff: &Vec<u8>) -> GeminiResponse {
    if res_buff.len() < 4 {
        panic!("Bad gemini response. Missing status status and end of line");
    }

    let status_digit1 = res_buff.get(0).unwrap();
    let status_digit2 = res_buff.get(1).unwrap();
    let status = GeminiStatus::from_u8(((status_digit1 - 0x30) * 10) + (status_digit2 - 0x30))
        .expect("unknown status code");

    let mut start_response_index = 2;
    let mut end_response_index = 0;
    if *res_buff.get(2).unwrap() == ' ' as u8 {
        start_response_index += 1;
    }
    for i in start_response_index..res_buff.len() - 1 {
        if *res_buff.get(i).unwrap() == '\r' as u8 && *res_buff.get(i + 1).unwrap() == '\n' as u8 {
            end_response_index = i;
            break;
        }
    }

    if end_response_index == 0 {
        panic!("Bad gemini response. Missing end of line");
    }

    let response_info =
        String::from_utf8_lossy(&res_buff[start_response_index..end_response_index]).to_string();
    let body = res_buff[end_response_index + 2..].to_vec();

    GeminiResponse::new(status, response_info, body)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GeminiGem {
    Text,
    LinkLine,
    Heading,
    ListItem,
    QuoteLine,
    PreformatToggle,
}

fn parse_document_to_gems(gemini_document: &String, request: &str) -> String {
    let lines = gemini_document.lines();
    let mut html_document = String::new();
    html_document.reserve(gemini_document.len());
    let mut gem_type = GeminiGem::Text;
    for line in lines {
        let mut peekable = line.chars().enumerate().peekable();

        if gem_type == GeminiGem::PreformatToggle {
            if line.starts_with("```") {
                println!("end pre");
                html_document.push_str("</pre>");
                gem_type = GeminiGem::Text;
                continue;
            }
            println!("{}", line);
            html_document.push_str(&format!("{}", line));
            continue;
        }
        if line.starts_with("```") {
            println!("start pre");
            html_document.push_str("<pre style=\"overflow: scroll;\">");
            gem_type = GeminiGem::PreformatToggle;
            continue;
        }

        match peekable.next() {
            Some((_, c)) => match c {
                '#' => {
                    if gem_type == GeminiGem::ListItem {
                        html_document.push_str("</ul>");
                    }
                    gem_type = GeminiGem::Heading;
                    let mut level = 1u8;
                    const MAX_LEVEL: u8 = 6;
                    while let Some((idx, next_char)) = peekable.next() {
                        if next_char == '#' {
                            if level == MAX_LEVEL {
                                continue;
                            }
                            level += 1;
                        } else if next_char == ' ' {
                            let text = html_escape::encode_text(&line[idx..]);

                            if level == 1 {
                                html_document.push_str(&format!("<h1>{}</h1>", text))
                            } else if level == 2 {
                                html_document.push_str(&format!("<h2>{}</h2>", text))
                            } else if level == 3 {
                                html_document.push_str(&format!("<h3>{}</h3>", text))
                            } else if level == 4 {
                                html_document.push_str(&format!("<h4>{}</h4>", text))
                            } else if level == 5 {
                                html_document.push_str(&format!("<h5>{}</h5>", text))
                            } else if level == 6 {
                                html_document.push_str(&format!("<h6>{}</h6>", text))
                            }
                            break;
                        }
                    }
                    continue;
                }
                '=' => {
                    if let Some((_, '>')) = peekable.peek() {
                        if gem_type == GeminiGem::ListItem {
                            html_document.push_str("</ul>");
                        }
                        gem_type = GeminiGem::LinkLine;
                        peekable.next();
                        if let Some((idx, ' ')) = peekable.peek() {
                            let text = html_escape::encode_text(&line[*idx..]);
                            let (url_str, text) = decode_link_line(&text);
                            if let Ok(url) = Url::parse(&url_str) {
                                if url.scheme() == "http" || url.scheme() == "https" {
                                    html_document.push_str(&format!(
                                        "<a href=\"{}\">{}</a><br>",
                                        url_str, text
                                    ));
                                } else {
                                    html_document.push_str(&format!(
                                        "<a href=\"?search={}\">{}</a><br>",
                                        url_str, text
                                    ));
                                }
                            } else {
                                if url_str.starts_with("/") {
                                    html_document.push_str(&format!(
                                        "<a href=\"?search={}{}\">{}</a><br>",
                                        request, url_str, text
                                    ));
                                } else {
                                    html_document.push_str(&format!(
                                        "<a href=\"?search={}/{}\">{}</a><br>",
                                        request, url_str, text
                                    ));
                                }
                            }
                        }
                    }
                    continue;
                }

                '*' => {
                    if let Some((idx, ' ')) = peekable.peek() {
                        if gem_type != GeminiGem::ListItem {
                            html_document.push_str("<ul>");
                            gem_type = GeminiGem::ListItem;
                        }
                        let text = html_escape::encode_text(&line[*idx..]);
                        html_document.push_str(&format!("<li>{}</li>", text))
                    }
                    continue;
                }

                '>' => {
                    if let Some((idx, ' ')) = peekable.peek() {
                        if gem_type == GeminiGem::ListItem {
                            html_document.push_str("</ul>");
                        }
                        gem_type = GeminiGem::QuoteLine;

                        let text = html_escape::encode_text(&line[*idx..]);
                        html_document.push_str(&format!("<blockquote>{}</blockquote>", text))
                    }
                    continue;
                }
                _ => {}
            },
            None => {}
        }
        if gem_type == GeminiGem::ListItem {
            html_document.push_str("</ul>");
            gem_type = GeminiGem::ListItem;
        }

        html_document.push_str(&format!("<p>{}</p>", html_escape::encode_text(line)));
    }

    return html_document;
}

fn decode_link_line(link_line: &str) -> (String, String) {
    let mut start_link: usize = 0;
    let mut end_link: usize = 0;
    let mut start_text: usize = 0;
    let mut start_empty = false;
    for (index, c) in link_line.chars().enumerate() {
        if c.is_whitespace() && index == 0 {
            start_empty = true;
        }

        if !c.is_whitespace() && start_empty {
            start_empty = false;
            start_link = index;
        }

        if c.is_whitespace() && end_link == 0 && !start_empty {
            end_link = index;
        }
        if !c.is_whitespace() && end_link > 0 && !start_empty {
            start_text = index;
            break;
        }
    }

    if end_link == 0 {
        end_link = link_line.len()
    }
    let link_str = link_line[start_link..end_link].to_string();
    let text_str = link_line[start_text..link_line.len() - 1].to_string();
    return (link_str, text_str);
}
