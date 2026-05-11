mod protocol;
use crate::protocol::StartTurnArgs;
use crate::protocol::StartMatchArgs;

use anyhow::Context;
use futures_util::{SinkExt, StreamExt, stream::SplitSink};
use serde::{Deserialize, Serialize};
use std::net::TcpStream;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream, connect_async, tungstenite::Message};

#[derive(Debug, Serialize, Deserialize)]
pub struct WebSocketMessage {
    command: Command,
    args: serde_json::Value,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Command {
    Hello,
    Login,
    Error,
    Ready,
    Practice,
    StartMatch,
    StartTurn,
    Move,
}

async fn send_command<
    S: SinkExt<Message, Error = tokio_tungstenite::tungstenite::Error> + Unpin,
>(
    write: &mut S,
    msg: WebSocketMessage,
) -> anyhow::Result<()> {
    let msg_deserialized = serde_json::to_string(&msg).context("serialize message")?;
    write
        .send(Message::Text(msg_deserialized.into()))
        .await
        .context("send message")?;
    Ok(())
}

#[tokio::main]
async fn main() {
    let url = "wss://bitdefenders.cvjd.me/ws";
    let (ws, _) = connect_async(url).await.unwrap();
    let (mut write, mut read) = ws.split();
    let mut my_id: Option<i32>=None;

    println!("connected");

    while let Some(msg) = read.next().await {
        let msg = msg.unwrap();
             let text = match msg {
            Message::Text(text) => text,
            Message::Ping(payload) => {
                write.send(Message::Pong(payload)).await.unwrap();
                continue;
            }
            Message::Pong(_) => {
                println!("pong");
                continue;
            }
            Message::Binary(_) => {
                println!("binary message ignored");
                continue;
            }
            Message::Close(frame) => {
                println!("closed: {frame:?}");
                break;
            }
            Message::Frame(_) => continue,
        };
        let message: WebSocketMessage = serde_json::from_str(&text).unwrap();
        println!("{message:?}");
        match message.command {
            Command::Hello => {
                // Send login
                if let Err(e) = send_command(
                    &mut write,
                    WebSocketMessage {
                        command: Command::Login,
                        args: serde_json::json!({"version": 1, "name": "Mira-Lupu"}),
                    },
                )
                .await {
                    println!("Failed to send login command: {e}");
                    break;
                }
            }
            Command::Login => {
                panic!("What are you doing here?");
            },
            Command::Error => {
                println!("Error: {message:?}");
                break;
            }

            Command::Practice => {
                panic!("Not yet!");
            }

            Command::Ready => {
                println!("You are ready to play!");
                //send practice
                if let Err(e) = send_command(
                    &mut write,
                    WebSocketMessage {
                        command: Command::Practice,
                        args: serde_json::json!({}),
                    },
                )
                .await {
                    println!("Failed to send practice command: {e}");
                    break;
                }
            },
            
            Command::StartMatch => {
                println!("Match starting!");
                
                let args: StartMatchArgs = serde_json::from_value(message.args.clone()).unwrap();
                my_id = Some(args.your_player_id);

                    println!("My id: {:?}", my_id);
            }

            Command::StartTurn => {
                println!("Start turn!");

                let args: StartTurnArgs = serde_json::from_value(message.args.clone()).unwrap();
                for hero in args.state.heroes{
                    println!("Hero {}: owner {}, at ({}, {}), HP: {}", hero.id, hero.owner_id, hero.x, hero.y, hero.hp);
                }
            /*    let (cx, cy)=(Hero("x"), Hero("y"));
                let (tx, ty)=(20, 20);
                let (sx, sy)=(sign(tx-cx), sign(ty-cy));
                let (cx, cy)=(cx+3*sx, cy+3*sy);
                //send turn
                if let Err(e) = send_command(
                    &mut write,
                    WebSocketMessage {
                        command: Command::Move,
                        args: serde_json::json!({"version": 1, "name": "Mira-Lupu"}),
                    },
                )
                .await {
                    println!("Failed to send move command: {e}");
                    break;
                }
            }*/}
            Command::Move => {
                println!("Not now!");
            }
        }
    }
}
