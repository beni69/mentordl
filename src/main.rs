#[macro_use]
extern crate log;
use clap::Parser;
use isahc::{
    cookies::{Cookie, CookieJar},
    http::Uri,
    prelude::*,
    Error, Request,
};
use serde_derive::Deserialize;
use std::{
    fs,
    path::PathBuf,
    process::{exit, Command},
    time::SystemTime,
};

const HOST: &str = "https://szlginfo.ptamas.hu";

#[derive(Parser, Debug)]
#[clap(name = "mentordl", author = "beni69",version = env!("CARGO_PKG_VERSION"))]
struct Args {
    #[clap(name = "url", help = "URL to download")]
    url: String,
    #[clap(name = "output dir", help = "Output directory")]
    dir: String,

    #[clap(name = "force", help = "Overwrite existing directory", long, short)]
    force: bool,
}

#[derive(Debug, Deserialize)]
struct Conf {
    username: String,
    password: String,
}

fn main() -> Result<(), Error> {
    // setup logger
    pretty_env_logger::init();
    info!("starting up..");

    let args = Args::parse();
    debug!("{:?}", args);

    let file_url = &args.url;
    let file_path = &args.dir;

    // parse config file
    let mut conf_file = PathBuf::from(dirs::config_dir().unwrap());
    conf_file.push("mentordl.txt");
    info!("looking for config at {:?}", &conf_file);
    let conf_str = match fs::read_to_string(&conf_file) {
        Ok(f) => f,
        Err(_) => {
            eprintln!(
                "Config file not found. Put your username and password in this file: {:?}",
                &conf_file
            );
            exit(1);
        }
    };
    let conf: Conf = match toml::from_str(&conf_str) {
        Ok(c) => c,
        Err(_) => {
            eprintln!("Config file at {:?} is invalid. Please do:\nusername = 'your username'\npassword = 'your password'", &conf_file);
            exit(1);
        }
    };
    info!("{:?}", &conf);

    // check if directory exists
    if PathBuf::from(file_path).exists() {
        if args.force {
            fs::remove_dir_all(file_path).expect("failed to remove directory");
        } else {
            eprintln!(
                "Directory already exists: {:?}\nRun again with the --force option to overwrite",
                &file_path
            );
            exit(1);
        }
    }

    // create a session
    let cookie_jar = CookieJar::new();
    let uri: Uri = "https://szlginfo.ptamas.hu/login.php".parse().unwrap();
    let mut res1 = Request::get(&uri)
        .cookie_jar(cookie_jar.clone())
        .body(())?
        .send()?;

    // parse the session data
    let res1_text = res1.text()?;
    let csrf_pos = res1_text
        .match_indices("const csrf = '")
        .next()
        .expect("csrf not found")
        .0;
    info!("{:?}", &csrf_pos);
    let csrf = &res1_text[csrf_pos + 14..csrf_pos + 14 + 64];
    info!("{:?}", &csrf);

    let sess: Cookie = cookie_jar
        .get_for_uri(&HOST.parse().unwrap())
        .into_iter()
        .find(|c| c.name() == "PHPSESSID")
        .unwrap();
    info!("{:?}", &sess);

    // login
    let uri2: Uri = format!("{}/backend/login.php", HOST).parse().unwrap();
    let mut _res2 = Request::post(&uri2)
        .cookie_jar(cookie_jar.clone())
        .header("Content-Type", "application/x-www-form-urlencoded")
        .body(format!(
            "username={}&password={}&csrf={}",
            &conf.username, &conf.password, csrf
        ))?
        .send()?;

    // download the file
    let mut url_file = url::Url::parse(file_url).expect("invalid url");
    let query = url_file.query().expect("invalid url");
    let mut q_str = String::new();
    for q in query.split('&') {
        let q = &q.replace("\\", ""); // some terminals do some weird character escaping
        if q.starts_with("csrf") {
            q_str.push_str(&format!("csrf={}", &csrf));
            q_str.push('&');
        } else {
            q_str.push_str(q);
            q_str.push('&');
        }
    }
    info!("{}", &q_str[..q_str.len() - 1]);
    url_file.set_query(Some(&q_str[..q_str.len() - 1]));
    let uri_file: Uri = url_file.as_str().parse().unwrap();
    debug!("{:?}", &uri_file);
    let mut _res_file = Request::get(&uri_file)
        .cookie_jar(cookie_jar.clone())
        .body(())?
        .send()?;

    let zipname = format!("mentordl_tmp_{}_{}.zip", &file_path, get_unix_time());
    _res_file.copy_to_file(&zipname)?;

    // unzip the file
    #[cfg(unix)]
    {
        exec(format!("unzip {} -d {}", &zipname, &file_path));
    }
    #[cfg(windows)]
    {
        fs::create_dir(file_path)?;
        exec(format!("tar.exe -xvf {} -C {}", &zipname, &file_path));
    }

    // delete original zip file
    fs::remove_file(&zipname).expect("failed to remove zip file, but everyehing else went fine.");

    Ok(())
}
fn exec(cmd: String) {
    info!("executing command: {}", &cmd);
    let cmd = string_to_cmd_and_args(&cmd);
    let out = Command::new(cmd.0)
        .args(cmd.1)
        .status()
        .expect("failed to spawn unzip command");
    info!("zip extractor exited with {}", out);
}
pub fn string_to_cmd_and_args(s: &str) -> (&str, Vec<&str>) {
    let v = s.split_ascii_whitespace().collect::<Vec<&str>>();
    let first = v.split_first().unwrap_or((&"", &[]));
    (*first.0, first.1.to_vec())
}
pub fn get_unix_time() -> u128 {
    match SystemTime::now().duration_since(SystemTime::UNIX_EPOCH) {
        Ok(t) => t.as_millis(),
        Err(_) => 0,
    }
}
