use serde::Deserialize;
use serde_json::from_str;
use std::env;
use std::path::Path;
use std::fs;
use std::process::Command;
use std::thread;
use std::time::Duration;
use std::str;

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

    let mysql_drop_db = env::var("MYSQL_DROP_DB").expect("MYSQL_DROP_DB must be set");

    if fs::metadata(format!("mysql/{}.sql", register_name)).is_ok() {
        println!("Dropping external database {}...", register_name);
        Command::new("mysql")
            .arg("<")
            .arg(&mysql_drop_db)
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

fn clean_local_credentials() -> std::io::Result<()> {
    println!("Cleaning up local credentials...");

    if fs::metadata("mysql/.my.cnf").is_ok() {
        fs::remove_file("mysql/.my.cnf")?;
        fs::remove_file(format!("{}/.my.cnf", env::var("HOME").unwrap()))?;
    } else if fs::metadata(format!("{}/.my.cnf", env::var("HOME").unwrap())).is_ok() {
        fs::remove_file(format!("{}/.my.cnf", env::var("HOME").unwrap()))?;
    }

    Ok(())
}

fn setup_local_database() -> std::io::Result<()> {
    println!("Database setup...");
    println!("Setting up mysql in env...");

    // let mysql_setup = env::var("MYSQL_SETUP").expect("MYSQL_SETUP must be set");
    // println!("Running mysql setup\n\n");
    // Command::new(mysql_setup)
    //     .status()
    //     .expect("Failed to execute command");

    if fs::metadata("mysql/data").is_err() {
        println!("No database found. Creating...");
        Command::new("mysqlinit")
            .status()
            .expect("Failed to execute command");
    } else {
        println!("Local database already setup. Continuing...");
    }

    println!("setting up mysqlcred...");
    Command::new("mysqlcred")
        .status()
        .expect("Failed to execute command");

    Ok(())
}

fn setup_external_database(register_name: &str) -> std::io::Result<()> {
    println!("setting up mysqlcred...");

    Command::new("mysqlcred")
        .status()
        .expect("Failed to execute command");

    if fs::metadata(format!("mysql/{}.sql", register_name)).is_err() {
        println!("No database found. Creating...");
        println!("Setting up root...");

        let mysql_create_db = env::var("MYSQL_CREATE_DB").expect("MYSQL_CREATE_DB must be set");
        Command::new("mysql")
            .arg("<")
            .arg(mysql_create_db)
            .status()
            .expect("Failed to execute command");

        println!("Creating database {}...", register_name);
        fs::File::create(format!("mysql/{}.sql", register_name))?;
    } else {
        println!("Local database already setup. Continuing...");

        let mysql_local_load_file = env::var("MYSQL_LOCAL_LOAD_FILE").expect("MYSQL_LOCAL_LOAD_FILE must be set");
        Command::new("mysql")
            .arg("<")
            .arg(mysql_local_load_file)
            .status()
            .expect("Failed to execute command");
    }

    Ok(())
}

fn start_database() -> std::io::Result<()> {
    let mysql_local_load_file = env::var("MYSQL_LOCAL_LOAD_FILE").expect("MYSQL_LOCAL_LOAD_FILE must be set");

    if fs::metadata("mysql/socket.lock").is_err() && Command::new("pgrep").arg("mysqld").output()?.stdout.is_empty() {
        println!("Starting MySQL as no socket.lock file and MySQL is not running...");
        Command::new("start_mysql")
            .status()
            .expect("Failed to execute command");

        println!("Setting load local inline files permissions...");
        Command::new("mysql")
            .arg("-S")
            .arg("MYSQL_UNIX_PORT")
            .arg("<")
            .arg(&mysql_local_load_file)
            .status()
            .expect("Failed to execute command");

        thread::sleep(Duration::from_secs(3));
    } else if fs::metadata("mysql/socket.lock").is_ok() && !Command::new("pgrep").arg("mysqld").output()?.stdout.is_empty() {
        println!("socket.lock file exists and MySQL is running. Continuing...");
        println!("Setting load local inline files permissions...");
        Command::new("mysql")
            .arg("-S")
            .arg("MYSQL_UNIX_PORT")
            .arg("<")
            .arg(&mysql_local_load_file)
            .status()
            .expect("Failed to execute command");
    } else if fs::metadata("mysql/socket.lock").is_err() && !Command::new("pgrep").arg("mysqld").output()?.stdout.is_empty() {
        println!("MySQL is running, but no socket.lock file found. Killing MySQL and restarting...");
        Command::new("pkill")
            .arg("mysqld")
            .status()
            .expect("Failed to execute command");

        thread::sleep(Duration::from_secs(3));
        Command::new("start_mysql")
            .status()
            .expect("Failed to execute command");

        println!("Setting load local inline files permissions...");
        Command::new("mysql")
            .arg("-S")
            .arg("MYSQL_UNIX_PORT")
            .arg("<")
            .arg(&mysql_local_load_file)
            .status()
            .expect("Failed to execute command");

        thread::sleep(Duration::from_secs(3));
    }

    Ok(())
}

fn compile_maven() -> std::io::Result<()> {
    if fs::metadata("target").is_err() {
        println!("No target directory found...");
        Command::new("mvn")
            .arg("clean")
            .arg("install")
            .arg("-DskipTests")
            .status()
            .expect("Failed to execute command");
    } else {
        println!("Target directory found. Cleaning up...");
        fs::remove_dir_all("target")?;
        Command::new("mvn")
            .arg("clean")
            .arg("package")
            .arg("-DskipTests")
            .status()
            .expect("Failed to execute command");
    }

    Ok(())
}

fn start_tomcat(register_name: &str) -> std::io::Result<()> {
    println!("Local environment detected...");
    println!("Setting up Tomcat...");

    let catalina_home = env::var("CATALINA_HOME").unwrap();
    fs::remove_file(format!("{}/webapps/{}.war", catalina_home, register_name))?;
    fs::remove_dir_all(format!("{}/webapps/{}", catalina_home, register_name))?;

    println!("Deploying new WAR...");
    fs::copy(format!("target/{}.war", register_name), format!("{}/webapps/{}.war", catalina_home, register_name))?;

    println!("Starting Tomcat...");
    Command::new(format!("{}/bin/start_tomcat", catalina_home))
        .status()
        .expect("Failed to execute command");

    Ok(())
}

fn copy_db_files() -> std::io::Result<()> {
    let catalina_home = env::var("CATALINA_HOME").unwrap();
    let db_path = format!("{}/bin/src/main/resources/db/application", catalina_home);

    if !Path::new(&db_path).exists() {
        fs::create_dir_all(&db_path)?;
    }

    println!("Copying db files...");
    Command::new("cp")
        .arg("-r")
        .arg("./src/main/resources/db/application/")
        .arg(&db_path)
        .status()
        .expect("Failed to execute command");

    Ok(())
}

fn main() {
    let args: Vec<String> = env::args().collect();

    let output = Command::new("nix-instantiate")
        .arg("--eval")
        .arg("--json")
        .arg("register.nix")
        .output()
        .expect("Failed to execute command");

    let output_str = str::from_utf8(&output.stdout).unwrap();
    let register: Register = from_str(output_str).expect("Failed to parse JSON");
    let register_name = register.register_name;

    match args.get(1).map(String::as_str) {
        Some("local") => {
            println!("Stopping running services...");
            Command::new("stop_tomcat").status().expect("Failed to execute command");
            clean_local_credentials().unwrap();
            setup_local_database().unwrap();
            start_database().unwrap();
            compile_maven().unwrap();
            start_tomcat(&register_name).unwrap();
            println!("\nFinished setting up local environment...");
        }
        Some("code") => {
            println!("Stopping running services...");
            Command::new("stop_tomcat").status().expect("Failed to execute command");
            clean_local_credentials().unwrap();
            setup_external_database(&register_name).unwrap();
            compile_maven().unwrap();
            start_tomcat(&register_name).unwrap();
            println!("\nFinished setting up environment for VScode...");
        }
        Some("docker") => {
            println!("Stopping running services...");
            Command::new("docker-compose").arg("down").status().expect("Failed to execute command");
            clean_local_credentials().unwrap();
            setup_local_database().unwrap();
            start_database().unwrap();
            compile_maven().unwrap();
            Command::new("docker").arg("build").arg("-t").arg(format!("{}:latest", register_name)).status().expect("Failed to execute command");
            println!("Starting docker-compose...");
            Command::new("docker-compose").arg("up").arg("-d").status().expect("Failed to execute command");
            println!("\nFinished setting up environment for Docker...");
        }
        Some("clean") => {
            clean_up(&register_name).unwrap();
            std::process::exit(0);
        }
        Some("drop") => {
            clean_up(&register_name).unwrap();
            drop_database(&register_name).unwrap();
            std::process::exit(0);
        }
        _ => {
            clean_local_credentials().unwrap();
            setup_external_database(&register_name).unwrap();
            compile_maven().unwrap();
            copy_db_files().unwrap();
            println!("\nFinished setting up environment for IntelliJ...");
            println!("Run tomcat server!");
        }
    }
}
