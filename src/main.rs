use std::path::Path;
use clap::Parser;
use hyper::{Body, HeaderMap, Method};
use url::Url;
use crate::WebClient;
use crate::Result;
mod client;
use client::*;
mod cli;


use cli::*;
use std::env::current_dir;
use std::ffi::OsStr;
use std::str::FromStr;
use hyper::body::HttpBody;
use hyper::header::{HeaderName, HeaderValue};
use serde_json;
use serde_json::{json, Value};

pub fn read_json_to_header_map(path: String) -> Result<HeaderMap>{
    let mut headers = HeaderMap::new();
    let data = std::fs::read_to_string(&*path);
    if let Ok(s) = data{
        let json: serde_json::error::Result<Value> = serde_json::from_str(s.as_str());
        if let Err(e) = json{
            return Err(Box::from(e))
        }
        let jmap = json.unwrap().as_object().unwrap().clone();
        for (key, value) in jmap.into_iter(){
            headers.insert(HeaderName::from_str(key.as_str()).unwrap(), HeaderValue::from_str(value.as_str().unwrap()).unwrap());
        }
        return Ok(headers)
    }
    Err(Box::from(data.unwrap_err()))
}

fn headermap_to_json(map: HeaderMap) -> String{
    let mut jmap = serde_json::map::Map::new();
    for (key, val) in map.into_iter(){
        jmap.insert(key.unwrap().to_string(), Value::from(val.to_str().unwrap()));
    }
    serde_json::to_string_pretty(&jmap).unwrap()
}

fn https_check(url: String) -> String{
    let mut newurl = String::from("https://");
    if !url.contains(&"http://".to_string()) && !url.contains(&"https://".to_string()){
        newurl.push_str(url.as_str());
        return newurl
    }
    url
}

fn cmd_to_method(cmd: &Commands) -> Method {
    let mut method = Method::GET;
    match cmd {
        Commands::GET { .. } =>{
            method = Method::GET
        },
        Commands::PUT { .. } =>{
            method = Method::PUT
        },
        Commands::POST { .. } =>{
            method = Method::POST
        },
        Commands::DELETE { .. } =>{
            method = Method::DELETE
        }
        Commands::OPTIONS { .. } =>{
            method = Method::OPTIONS
        },
        Commands::HEAD { .. } =>{
            method = Method::HEAD
        },
        Commands::CONNECT { .. } =>{
            method = Method::CONNECT
        },
        Commands::PATCH { .. } =>{
            method = Method::PATCH
        },
        Commands::TRACE { .. } =>{
            method = Method::TRACE
        },
        _ => {}
    }
    method
}

#[tokio::main]
async fn main() -> Result<()> {
    let client = WebClient::new();
    let cmd = Cli::parse();
    let mut map = HeaderMap::new();
    if let Some(path) = cmd.json_header_path{
        let tmp = read_json_to_header_map(path);
        if let Ok(m) = tmp{
            map = m.clone();
        }
        else{
            return Err(Box::from(tmp.unwrap_err().to_string()));
        }
    }
    let method = cmd_to_method(&cmd.command);
    match cmd.command {
        Commands::Download {outpath, url} => {
            let url = https_check(url);
            let mut follow_redirect = true;
            if cmd.no_redirect{
                follow_redirect = false
            }
            let resp = client.send_request(&url, Method::GET, Vec::new(), map.clone(), follow_redirect, true).await;
            let parse = Url::parse(&*url);
            let cwd = current_dir().unwrap();
            let mut path = String::new();
            if outpath.is_none(){
                if let Ok(p) = parse{
                    let url_path = client.url_to_file_path(p.path().to_string());
                    let file_name = Path::new(&url_path).file_name();
                    if let Some(name) = file_name{
                        path = cwd.join(name.to_str().unwrap()).to_str().unwrap().to_string();
                    }
                    else{
                        path = p.host_str().unwrap().to_string();
                        path.push_str(".html");
                    }
                }
            }
            else{
                path = outpath.unwrap();
            }
            match resp {
                Err(e) => {
                    eprintln!("{}", e.to_string());
                }
                Ok(mut r) => {
                    let length = r.size_hint().upper().unwrap_or(0) as f64;
                    let bytes = hyper::body::to_bytes(r.body_mut()).await?;
                    let f = std::fs::write(&path, bytes);
                    if f.is_ok(){
                        println!("Downloaded {} with {} bytes to {}", url, length, path);
                    }
                    else{
                        println!("{:?}", f.unwrap_err().to_string());
                    }
                }
            }
            return Ok(())
        },
        Commands::SiteDownload {url, outputdir, level} => {
            let url = https_check(url);
            let mut path = String::new();
            if outputdir.is_none(){
                path = current_dir().unwrap().to_str().unwrap().to_string();
            }
            else{
                path = outputdir.unwrap();
            }
            let resp = client.download(&url, map, level, &path).await;
            if let Err(e) = resp{
                eprintln!("{}", e.to_string());
            }
            else{
                println!("Downloaded {} to {}", url, path);
            }
            return Ok(())
        }
        Commands::GET {url} | Commands::POST {url} | Commands::PUT {url} | Commands::CONNECT {url}  | Commands::PATCH {url} | Commands::DELETE {url} | Commands::OPTIONS {url} | Commands::HEAD {url} | Commands::TRACE {url} =>{
            let url = https_check(url);
            let mut file_data:Vec<u8> = Vec::new();
            let mut follow_redirect = true;
            if let Some(path) = cmd.file_path{
                let bytes = std::fs::read(path);
                if let Ok(b) = bytes{
                    file_data = b;
                }
                else{
                    let e = bytes.unwrap_err();
                    return Err(Box::from(e.to_string()))
                }
            }
            if cmd.no_redirect{
                follow_redirect = false
            }
            let mut resp = client.send_request(&url, method, file_data, map, follow_redirect, true).await?;
            println!("\n\n");
            if let Some(header_path) = cmd.dump_headers{
                let dump = std::fs::write(&header_path, headermap_to_json(resp.headers().clone()));
                if dump.is_ok(){
                    println!("Wrote headers to {}", header_path);
                }
                else{
                    println!("{}", dump.unwrap_err().to_string());
                }
            }
            if cmd.info{
                println!("\nResponse headers: {}", headermap_to_json(resp.headers().clone()));
                println!("\nResponse status: {}", resp.status());
                println!("\nHttp version: {:?}", resp.version());
            }
            else{
                let bytes = hyper::body::to_bytes(resp.body_mut()).await?;
                let data_string = String::from_utf8(bytes.to_vec());
                if let Err(e) = data_string{
                    return Err(Box::from(e.to_string()))
                }
                let data_string = data_string.unwrap();
                println!("\nResponse data: {}", data_string);
            }
        },
    }


    Ok(())
}
