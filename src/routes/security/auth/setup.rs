use rocket::form::Form;
use rocket::response::Redirect;
use rocket::serde::json::Json;
use rocket::State;
use rocket_dyn_templates::Template;
use serde::{Deserialize, Serialize};

use crate::security::auth;
use crate::db::DbPool;
use crate::models::settings::Setting;
use crate::AdminSlug;

use super::super::NoCacheTemplate;
use super::login::needs_setup;

#[derive(Debug, Serialize)]
struct SetupContext {
    error: Option<String>,
    admin_slug: String,
    site_name: String,
    admin_email: String,
    db_backend: String,
    mongo_uri: String,
    mongo_db_name: String,
    mongo_auth_enabled: String,
    mongo_auth_mechanism: String,
    mongo_username: String,
    mongo_password: String,
    mongo_auth_db: String,
}

#[derive(Debug, FromForm, Deserialize)]
pub struct SetupForm {
    pub db_backend: String,
    pub mongo_uri: Option<String>,
    pub mongo_db_name: Option<String>,
    pub mongo_auth_enabled: Option<String>,
    pub mongo_auth_mechanism: Option<String>,
    pub mongo_username: Option<String>,
    pub mongo_password: Option<String>,
    pub mongo_auth_db: Option<String>,
    pub site_name: String,
    pub admin_email: String,
    pub password: String,
    pub confirm_password: String,
    pub accept_terms: Option<String>,
}

#[get("/setup")]
pub fn setup_page(pool: &State<DbPool>, admin_slug: &State<AdminSlug>) -> Result<NoCacheTemplate, Redirect> {
    if !needs_setup(pool) {
        return Err(Redirect::to(format!("/{}/login", admin_slug.0)));
    }
    let ctx = SetupContext {
        error: None,
        admin_slug: admin_slug.0.clone(),
        site_name: "Velocty".to_string(),
        admin_email: String::new(),
        db_backend: "sqlite".to_string(),
        mongo_uri: "mongodb://localhost:27017".to_string(),
        mongo_db_name: "velocty".to_string(),
        mongo_auth_enabled: String::new(),
        mongo_auth_mechanism: "scram_sha256".to_string(),
        mongo_username: String::new(),
        mongo_password: String::new(),
        mongo_auth_db: "admin".to_string(),
    };
    Ok(NoCacheTemplate(Template::render("admin/setup", &ctx)))
}

#[post("/setup", data = "<form>")]
pub fn setup_submit(
    form: Form<SetupForm>,
    pool: &State<DbPool>,
    admin_slug: &State<AdminSlug>,
) -> Result<Redirect, Template> {
    if !needs_setup(pool) {
        return Ok(Redirect::to(format!("/{}/login", admin_slug.0)));
    }

    let make_err = |msg: &str, form: &SetupForm| {
        let ctx = SetupContext {
            error: Some(msg.to_string()),
            admin_slug: admin_slug.0.clone(),
            site_name: form.site_name.clone(),
            admin_email: form.admin_email.clone(),
            db_backend: form.db_backend.clone(),
            mongo_uri: form.mongo_uri.clone().unwrap_or_default(),
            mongo_db_name: form.mongo_db_name.clone().unwrap_or_default(),
            mongo_auth_enabled: form.mongo_auth_enabled.clone().unwrap_or_default(),
            mongo_auth_mechanism: form.mongo_auth_mechanism.clone().unwrap_or_else(|| "scram_sha256".to_string()),
            mongo_username: form.mongo_username.clone().unwrap_or_default(),
            mongo_password: form.mongo_password.clone().unwrap_or_default(),
            mongo_auth_db: form.mongo_auth_db.clone().unwrap_or_else(|| "admin".to_string()),
        };
        Template::render("admin/setup", &ctx)
    };

    // Validate DB backend
    if form.db_backend != "sqlite" && form.db_backend != "mongodb" {
        return Err(make_err("Please select a database backend.", &form));
    }
    if form.db_backend == "mongodb" {
        let uri = form.mongo_uri.as_deref().unwrap_or("").trim();
        let db_name = form.mongo_db_name.as_deref().unwrap_or("").trim();
        if uri.is_empty() {
            return Err(make_err("MongoDB connection URI is required.", &form));
        }
        if db_name.is_empty() {
            return Err(make_err("MongoDB database name is required.", &form));
        }
        // Validate auth fields if auth is enabled
        if form.mongo_auth_enabled.as_deref() == Some("true") {
            let mech = form.mongo_auth_mechanism.as_deref().unwrap_or("scram_sha256");
            // X.509 and AWS don't require username/password
            if mech != "x509" && mech != "aws" {
                if form.mongo_username.as_deref().unwrap_or("").trim().is_empty() {
                    return Err(make_err("MongoDB username is required for the selected auth mechanism.", &form));
                }
                if form.mongo_password.as_deref().unwrap_or("").trim().is_empty() {
                    return Err(make_err("MongoDB password is required for the selected auth mechanism.", &form));
                }
            }
        }
    }

    // Validate
    if form.admin_email.trim().is_empty() {
        return Err(make_err("Email is required.", &form));
    }
    if form.password.len() < 8 {
        return Err(make_err("Password must be at least 8 characters.", &form));
    }
    if form.password != form.confirm_password {
        return Err(make_err("Passwords do not match.", &form));
    }
    if form.accept_terms.as_deref() != Some("true") {
        return Err(make_err("You must accept the Terms of Use and Privacy Policy.", &form));
    }

    // Write velocty.toml with DB backend choice
    let toml_content = if form.db_backend == "mongodb" {
        let mut toml = format!(
            "[database]\nbackend = \"mongodb\"\nuri = \"{}\"\nname = \"{}\"\n",
            form.mongo_uri.as_deref().unwrap_or("mongodb://localhost:27017").trim(),
            form.mongo_db_name.as_deref().unwrap_or("velocty").trim(),
        );
        if form.mongo_auth_enabled.as_deref() == Some("true") {
            let mech = form.mongo_auth_mechanism.as_deref().unwrap_or("scram_sha256").trim();
            let auth_db = form.mongo_auth_db.as_deref().unwrap_or("admin").trim();
            toml.push_str(&format!("\n[database.auth]\nmechanism = \"{}\"\nauth_db = \"{}\"\n", mech, auth_db));
            let user = form.mongo_username.as_deref().unwrap_or("").trim();
            let pass = form.mongo_password.as_deref().unwrap_or("").trim();
            if !user.is_empty() {
                toml.push_str(&format!("username = \"{}\"\n", user));
            }
            if !pass.is_empty() {
                toml.push_str(&format!("password = \"{}\"\n", pass));
            }
        }
        toml
    } else {
        "[database]\nbackend = \"sqlite\"\npath = \"website/site/db/velocty.db\"\n".to_string()
    };
    if let Err(e) = std::fs::write("velocty.toml", &toml_content) {
        return Err(make_err(&format!("Failed to write config: {}", e), &form));
    }

    // Save
    let hash = auth::hash_password(&form.password)
        .map_err(|_| make_err("Failed to hash password.", &form))?;

    let _ = Setting::set(pool, "site_name", form.site_name.trim());
    let _ = Setting::set(pool, "admin_email", form.admin_email.trim());
    let _ = Setting::set(pool, "admin_password_hash", &hash);
    let _ = Setting::set(pool, "setup_completed", "true");
    let _ = Setting::set(pool, "db_backend", &form.db_backend);

    Ok(Redirect::to(format!("/{}/login", admin_slug.0)))
}

// ── MongoDB Connection Test ─────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct TestMongoForm {
    pub uri: String,
}

#[derive(Debug, Serialize)]
pub struct TestMongoResult {
    pub ok: bool,
    pub message: String,
}

/// Parse a mongodb:// or mongodb+srv:// URI and extract host:port for TCP test.
fn parse_mongo_host(uri: &str) -> Result<(String, u16), String> {
    let stripped = uri
        .strip_prefix("mongodb+srv://")
        .or_else(|| uri.strip_prefix("mongodb://"))
        .ok_or_else(|| "URI must start with mongodb:// or mongodb+srv://".to_string())?;

    // Remove credentials (user:pass@)
    let after_creds = if let Some(pos) = stripped.find('@') {
        &stripped[pos + 1..]
    } else {
        stripped
    };

    // Remove path and query (/dbname?options)
    let host_part = after_creds.split('/').next().unwrap_or(after_creds);
    // Take first host if replica set (host1:port,host2:port)
    let first_host = host_part.split(',').next().unwrap_or(host_part);

    if let Some(colon) = first_host.rfind(':') {
        let host = first_host[..colon].to_string();
        let port = first_host[colon + 1..]
            .parse::<u16>()
            .map_err(|_| "Invalid port in URI".to_string())?;
        Ok((host, port))
    } else {
        Ok((first_host.to_string(), 27017))
    }
}

#[post("/setup/test-mongo", format = "json", data = "<body>")]
pub fn test_mongo_connection(body: Json<TestMongoForm>) -> Json<TestMongoResult> {
    use std::io::{Read, Write};
    use std::net::TcpStream;
    use std::time::Duration;

    let uri = body.uri.trim();
    if uri.is_empty() {
        return Json(TestMongoResult {
            ok: false,
            message: "Connection URI is empty.".to_string(),
        });
    }

    let (host, port) = match parse_mongo_host(uri) {
        Ok(hp) => hp,
        Err(e) => {
            return Json(TestMongoResult {
                ok: false,
                message: format!("Invalid URI: {}", e),
            })
        }
    };

    let addr = format!("{}:{}", host, port);

    // Step 1: TCP connect with 5-second timeout
    let stream = match TcpStream::connect_timeout(
        &match addr.parse() {
            Ok(a) => a,
            Err(_) => {
                // Resolve hostname
                match std::net::ToSocketAddrs::to_socket_addrs(&addr.as_str()) {
                    Ok(mut addrs) => match addrs.next() {
                        Some(a) => a,
                        None => {
                            return Json(TestMongoResult {
                                ok: false,
                                message: format!("Cannot resolve host: {}", host),
                            })
                        }
                    },
                    Err(e) => {
                        return Json(TestMongoResult {
                            ok: false,
                            message: format!("Cannot resolve host '{}': {}", host, e),
                        })
                    }
                }
            }
        },
        Duration::from_secs(5),
    ) {
        Ok(s) => s,
        Err(e) => {
            return Json(TestMongoResult {
                ok: false,
                message: format!("Cannot connect to {}: {}", addr, e),
            })
        }
    };

    let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
    let _ = stream.set_write_timeout(Some(Duration::from_secs(5)));

    // Step 2: Send a minimal MongoDB OP_MSG isMaster command
    // This is the wire protocol way to verify it's actually MongoDB.
    // Build a BSON document: { isMaster: 1, $db: "admin" }
    let bson_doc: Vec<u8> = {
        let mut doc = Vec::new();
        // isMaster: 1 (int32, type 0x10)
        doc.push(0x10); // type: int32
        doc.extend_from_slice(b"isMaster\0");
        doc.extend_from_slice(&1i32.to_le_bytes());
        // $db: "admin" (string, type 0x02)
        doc.push(0x02); // type: string
        doc.extend_from_slice(b"$db\0");
        let db_val = b"admin\0";
        doc.extend_from_slice(&(db_val.len() as i32).to_le_bytes());
        doc.extend_from_slice(db_val);
        // terminator
        doc.push(0x00);
        // Prepend total length (4 bytes for length + doc)
        let total = (4 + doc.len()) as i32;
        let mut full = total.to_le_bytes().to_vec();
        full.extend_from_slice(&doc);
        full
    };

    // OP_MSG header (MongoDB 3.6+)
    let flag_bits: u32 = 0;
    let section_kind: u8 = 0; // body
    let msg_body_len = 4 + 1 + bson_doc.len(); // flagBits + sectionKind + document
    let total_msg_len = (16 + msg_body_len) as i32; // header(16) + body

    let request_id: i32 = 1;
    let response_to: i32 = 0;
    let op_code: i32 = 2013; // OP_MSG

    let mut msg = Vec::new();
    msg.extend_from_slice(&total_msg_len.to_le_bytes());
    msg.extend_from_slice(&request_id.to_le_bytes());
    msg.extend_from_slice(&response_to.to_le_bytes());
    msg.extend_from_slice(&op_code.to_le_bytes());
    msg.extend_from_slice(&flag_bits.to_le_bytes());
    msg.push(section_kind);
    msg.extend_from_slice(&bson_doc);

    let mut stream = stream;
    if let Err(e) = stream.write_all(&msg) {
        return Json(TestMongoResult {
            ok: false,
            message: format!("Failed to send handshake: {}", e),
        });
    }

    // Step 3: Read response header (at least 16 bytes)
    let mut header = [0u8; 16];
    match stream.read_exact(&mut header) {
        Ok(_) => {
            let resp_len = i32::from_le_bytes([header[0], header[1], header[2], header[3]]);
            let resp_op = i32::from_le_bytes([header[12], header[13], header[14], header[15]]);
            if resp_op == 2013 && resp_len > 16 {
                Json(TestMongoResult {
                    ok: true,
                    message: format!("Connected to MongoDB at {}", addr),
                })
            } else {
                Json(TestMongoResult {
                    ok: false,
                    message: format!("Server at {} responded but doesn't appear to be MongoDB (opcode: {})", addr, resp_op),
                })
            }
        }
        Err(e) => Json(TestMongoResult {
            ok: false,
            message: format!("Server at {} accepted connection but didn't respond: {}", addr, e),
        }),
    }
}
