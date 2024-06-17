mod bindings;

pub use bindings::wasi::http::types::{
    Fields, IncomingRequest, OutgoingBody, OutgoingResponse, ResponseOutparam,
};

struct Component;

bindings::export!(Component with_types_in bindings);

const SAMPLE_FILE_PATH: &str = "./sample.txt";

impl bindings::exports::wasi::http::incoming_handler::Guest for Component {
    fn handle(request: IncomingRequest, outparam: ResponseOutparam) {

        let hdrs = Fields::new();
        let resp = OutgoingResponse::new(hdrs);
        let body = resp.body().expect("outgoing response");

        ResponseOutparam::set(outparam, Ok(resp));

        println!("Hello on the server console!");

        // Get the output stream
        let out = body.write().expect("outgoing stream");

        // Write the request path to the response body
        let request_path = request.path_with_query().unwrap_or("no request query path".to_string());

        out.blocking_write_and_flush(format!("Hello, you pinged {request_path}!\n").as_bytes())
            .expect("writing response");

        // Write the file's contents to the response body
        let file_contents = std::fs::read_to_string(SAMPLE_FILE_PATH).expect("Can't read file!");
        out.blocking_write_and_flush(file_contents.as_bytes())
            .expect("writing response");

        drop(out);
        OutgoingBody::finish(body, None).unwrap();
    }
}
