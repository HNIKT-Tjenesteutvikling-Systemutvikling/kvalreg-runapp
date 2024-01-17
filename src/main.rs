use serde::Deserialize;
use serde_json::from_str;
use std::env;
use std::fs;
use std::process::Command;
use std::thread;
use std::time::Duration;

#[derive(Deserialize)]
struct Register {
    #[serde(rename = "registerName")]
    register_name: String,
}

fn clean_up(register_name: &str) -> std::io::Result<()> {
    println!("Cleaning up and stopping services...");

    Command::new("stop_tomcat")
        .status()
        .expect("Failed to execute command");

    Command::new("docker-compose")
        .arg("down")
        .status()
        .expect("Failed to execute command");

    println!("Cleaning up and stopping MySQL...");
    if fs::metadata("mysql/data").is_ok() {
        if fs::metadata("mysql/socket.lock").is_ok() {
            Command::new("stop_mysql")
                .status()
                .expect("Failed to execute command");

            let output = Command::new("pgrep")
                .arg("mysqld")
                .output()
                .expect("Failed to execute command");
            if !output.stdout.is_empty() {
                Command::new("pkill")
                    .arg("mysqld")
                    .status()
                    .expect("Failed to execute command");
            }
        }

        println!("\nAwaiting MySQL shutdown...");
        thread::sleep(Duration::from_secs(5));
        println!("\n\n\n");

        fs::remove_file(format!("mysql/.my.cnf"))?;
        fs::remove_file(format!("{}/.my.cnf", env::var("HOME").unwrap()))?;
        fs::remove_file(format!("target/{}.war", register_name))?;
        fs::remove_dir_all(format!("target/{}", register_name))?;
        fs::remove_file(format!(
            "{}/webapps/{}.war",
            env::var("CATALINA_HOME").unwrap(),
            register_name
        ))?;
        fs::remove_dir_all(format!(
            "{}/webapps/{}",
            env::var("CATALINA_HOME").unwrap(),
            register_name
        ))?;
        fs::remove_dir_all(format!("{}/bin/src/*", env::var("CATALINA_HOME").unwrap()))?;
        fs::remove_dir_all(format!("{}/logs/*", env::var("CATALINA_HOME").unwrap()))?;
        fs::remove_dir_all("jdk/*")?;
        fs::remove_dir_all("logs/*")?;

        println!("Stopped running processes");
    } else {
        println!("No local database found. Continuing...");
    }

    Ok(())
}

fn drop_database(register_name: &str) -> std::io::Result<()> {
    println!("Starting to drop database...");

    if fs::metadata(format!("mysql/{}.sql", register_name)).is_ok() {
        println!("Dropping external database {}...", register_name);
        Command::new("mysql")
            .arg("<")
            .arg("mysql_drop_db")
            .status()
            .expect("Failed to execute command");

        std::thread::sleep(std::time::Duration::from_secs(1));
        fs::remove_file(format!("mysql/{}.sql", register_name))?;
    }

    if fs::metadata("mysql/data").is_ok() {
        println!("Dropping local database...");
        fs::remove_dir_all("mysql/data")?;
    }

    if fs::metadata("mysql/.my.cnf").is_ok() {
        fs::remove_file("mysql/.my.cnf")?;
        fs::remove_file(format!("{}/.my.cnf", env::var("HOME").unwrap()))?;
    } else if fs::metadata(format!("{}/.my.cnf", env::var("HOME").unwrap())).is_ok() {
        fs::remove_file(format!("{}/.my.cnf", env::var("HOME").unwrap()))?;
    }

    if fs::metadata(format!("{}/bin/src", env::var("CATALINA_HOME").unwrap())).is_ok() {
        fs::remove_dir_all(format!("{}/bin/src/*", env::var("CATALINA_HOME").unwrap()))?;
        fs::remove_dir_all(format!("{}/logs/*", env::var("CATALINA_HOME").unwrap()))?;
        fs::remove_dir_all(format!("{}/webapps/*", env::var("CATALINA_HOME").unwrap()))?;
        fs::remove_dir_all("logs/*")?;
    }
    println!("Database dropped.");

    Ok(())
}

fn main() {
    let args: Vec<String> = env::args().collect();

    let register_json = fs::read_to_string("register.nix").expect("Failed to read file");
    let register: Register = from_str(&register_json).expect("Failed to parse JSON");
    let register_name = register.register_name;

    match args.get(1).map(String::as_str) {
        Some("drop") => {
            clean_up(&register_name).unwrap();
            drop_database(&register_name).unwrap();
        }
        _ => {}
    }
}
