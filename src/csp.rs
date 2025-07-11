use rocket::Response;
use rocket::fairing::{Fairing, Info, Kind};
use rocket::http::Header;
use rocket::request::Request;

pub struct CSPFairing;

#[rocket::async_trait]
impl Fairing for CSPFairing {
    fn info(&self) -> Info {
        Info {
            name: "Adds the CSP header",
            kind: Kind::Response,
        }
    }

    async fn on_response<'r>(&self, _request: &'r Request<'_>, response: &mut Response<'r>) {
        response.set_header(Header::new(
            "Content-Security-Policy",
            "default-src 'self'; script-src 'self' 'unsafe-inline'; style-src 'self' 'unsafe-inline'; img-src 'self' data:; font-src 'self'; object-src 'none'; base-uri 'self'; frame-ancestors 'none';",
        ));
    }
}
