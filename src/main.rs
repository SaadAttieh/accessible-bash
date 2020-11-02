
use actix::prelude::*;
use actix::AsyncContext;
use actix_files as fs;

use actix_web::{middleware, web, App, Error, HttpRequest, HttpResponse, HttpServer};
use actix_web_actors::ws;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt};

use tokio::process::{ChildStdin, Command};
//used to import Child, ChildStdin, ChildStdout,
use tokio::stream::StreamExt;

use tokio::io::BufReader;

use std::process::Stdio;

#[derive(Message)]
#[rtype(result = "Result<(), ()>")]
struct CommandRunner(String);

/// Define HTTP actor
struct MyWs {
    process: Option<ChildStdin>,
}

impl Actor for MyWs {
    type Context = ws::WebsocketContext<Self>;
}

#[derive(Debug)]
struct Line(String);

impl StreamHandler<Result<Line, ws::ProtocolError>> for MyWs {
    fn handle(&mut self, msg: Result<Line, ws::ProtocolError>, ctx: &mut Self::Context) {
        println!("sending message={:?}", msg);
        match msg {
            Ok(line) => ctx.text(line.0),
            _ => (), //Handle errors
        }
    }
}

/// Handler for ws::Message message
impl StreamHandler<Result<ws::Message, ws::ProtocolError>> for MyWs {
    fn handle(&mut self, msg: Result<ws::Message, ws::ProtocolError>, ctx: &mut Self::Context) {
        println!("Message={:?}", msg);
        match msg {
            Ok(ws::Message::Ping(msg)) => ctx.pong(&msg),
            Ok(ws::Message::Text(text)) => {
                ctx.notify(CommandRunner(text.to_string()));
            }
            Ok(ws::Message::Binary(bin)) => ctx.binary(bin),
            _ => (),
        }
    }
}

impl Handler<CommandRunner> for MyWs {
    type Result = Result<(), ()>;
    fn handle(&mut self, msg: CommandRunner, ctx: &mut Self::Context) -> Self::Result {
        if let Some(ref mut input) = self.process {
            let fut = input.write_all(msg.0.as_bytes());
            let fut = actix::fut::wrap_future::<_, Self>(fut);
            ctx.spawn(fut);

            return Ok(());
        }

        let mut cmd = Command::new(msg.0);
        cmd.stdout(Stdio::piped()).stdin(Stdio::piped());
        let mut child = cmd.spawn().expect("failed to spawn command");

        let stdout = child.stdout.take().unwrap();
        self.process = Some(child.stdin.take().unwrap());
        let reader = BufReader::new(stdout).lines();

        // Ensure the child process is spawned in the runtime so it can
        // make progress on its own while we await for any output.
        ctx.add_stream(reader.map(|l| Ok(Line(l.expect("Not a line")))));
        Ok(())
    }
}

async fn ws_index(req: HttpRequest, stream: web::Payload) -> Result<HttpResponse, Error> {
    let resp = ws::start(MyWs { process: None }, &req, stream);
    println!("{:?}", resp);
    resp
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    std::env::set_var("RUST_LOG", "actix_server=info,actix_web=info");
    env_logger::init();

    HttpServer::new(|| {
        App::new()
            // enable logger
            .wrap(middleware::Logger::default())
            // websocket route
            .service(web::resource("/ws/").route(web::get().to(ws_index)))
            // static files
            .service(fs::Files::new("/static", ".").show_files_listing())
    })
    // start http server on 127.0.0.1:8080
    .bind("127.0.0.1:8080")?
    .run()
    .await
}
