mod protocol;
use crate::protocol::StartTurnArgs;
use crate::protocol::StartMatchArgs;
use crate::protocol::EndMatchArgs;

use std::collections::{HashMap, HashSet, VecDeque};
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
    Challenge,
    StartMatch,
    StartTurn,
    Move,
    Shoot,
    EndMatch,
}

#[derive(Clone, Copy, Eq, PartialEq, Hash)]
struct Node {
    x: i32,
    y: i32,
}

const DIRECTIONS: [(i32, i32); 8] = [
    (-3, -3),
    (-3, 0),
    (-3, 3),
    (0, -3),
    (0, 3),
    (3, -3),
    (3, 0),
    (3, 3),
];

fn align(v: i32) -> i32 {
    let r = (v - 1) % 3;
    v - r
}

fn get_neighbors(node: Node, walls: &[protocol::Wall], projectiles: &[protocol::Projectile], width: i32, height: i32, id: Option<i32>) -> Vec<Node> {
    let mut result = Vec::new();
    for (dx, dy) in DIRECTIONS {
        let nx = node.x + dx;
        let ny = node.y + dy;
        if can_move(nx, ny, walls, projectiles, width, height, id) {
            result.push(Node { x: nx, y: ny });
        }
    }
    result
}

fn can_move(cx: i32, cy: i32, walls: &[protocol::Wall], projectiles: &[protocol::Projectile], width: i32, height: i32, id: Option<i32>) -> bool {
    for dx in -1..=1 {
        for dy in -1..=1 {
            let x = cx + dx;
            let y = cy + dy;
            if x < 0 || y < 0 || x >= width || y >= height {
                return false;
            }
            if walls.iter().any(|w| w.x == x && w.y == y) {
                return false;
            }
            for p in projectiles{
                 if Some(p.owner_id) != id{
                    if p.ttl>=0{
                        if p.x == x && p.y == y {
                            return false;
                        }
                    }
                 }
            } 
        }
    }

    true
}

fn bfs(start: Node, goal: Node, walls: &[protocol::Wall], projectiles: &[protocol::Projectile], width: i32, height: i32, id: Option<i32>) -> Option<Node> {
    let mut queue = VecDeque::new();
    let mut visited = HashSet::new();
    let mut came_from: HashMap<Node, Node> = HashMap::new();
    queue.push_back(start);
    visited.insert(start);
    while let Some(current) = queue.pop_front() {
        if current == goal {
            let mut node = current;
            while let Some(prev) = came_from.get(&node) {
                if *prev == start {
                    return Some(node);
                }
                node = *prev;
            }
            return None;
        }
        for neighbor in get_neighbors(current, walls, projectiles, width, height, id) {
            if !visited.contains(&neighbor) {
                visited.insert(neighbor);
                came_from.insert(neighbor, current);
                queue.push_back(neighbor);
            }
        }
    }
    None
}

fn shoot_verif(start: Node, e: Node, walls: &[protocol::Wall]) -> bool{
   if e.x==-1 && e.y==-1{
        println!("enemy already dead!");
        return false;
   }
   let mut dir= Node{
            x:0,
            y:0
        };
        dir.x=e.x-start.x;
        dir.y=e.y-start.y;
        let mut coords=Node{
            x:start.x,
            y:start.y,
        };
        if e.x==start.x && e.y==start.y{
            println!("Hero in front of enemy");
        }
            while coords.x!=e.x && coords.y!=e.y{
                println!("coords {} {} -> {} {}", coords.x, coords.y, e.x, e.y);
                if walls.iter().any(|w| w.x == coords.x && w.y == coords.y){
                    println!("Obstructed by wall!");
                    return false;
                } 
                coords.x=coords.x+dir.x.signum();
                coords.y=coords.y+dir.y.signum();
            }
            return true;
}

fn shoot (start: Node, walls: &[protocol::Wall], e1: Node, e2: Node) -> Node{
    let mut sgoal=Node{
        x:-1,
        y:-1
    };
    let d1=(e1.x-start.x+e1.y-start.y).abs();
    let d2=(e2.x-start.x+e2.y-start.y).abs();
    println!("Verif e1..");
    let ve1=shoot_verif(start, e1, walls);
    println!("Verif e2..");
    let ve2=shoot_verif(start, e2, walls);
    if d1<d2{
        if ve1 == true{
            println!("d1<d2 -> Decided with e1: {} {}", e1.x, e1.y);
            return e1;
        }
        if ve2 == true{
            println!("d1<d2 -> Decided with e2: {} {}", e2.x, e2.y);
            return e2;
        }
    }
    else{
        if ve2 == true{
            println!(" d1>d2 -> Decided with e2: {} {}", e2.x, e2.y);
            return e2;
        }
        if ve1 == true{
            println!("d1>d2 -> Decided with e1: {} {}", e1.x, e1.y);
            return e1;
        }
    }
    println!("No good target yet..");
    return sgoal;
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
    let mut width=0;
    let mut height=0;
    let mut goal = Node {
        x: align(0),
        y: align(0),
    };                                    
    let mut sgoal = Node {
        x: 0,
        y: 0,
    }; 
    let mut e1=Node{
        x:0,
        y:0,
    };
    let mut e2=Node{
        x:0,
        y:0,
    };
    let mut e1d=1000;
    let mut e2d=1000;
    let mut saw_enemy: bool=false;                                   
                        

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
            
            Command::Challenge => {
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
                width=args.config.width;
                height=args.config.height;
            }

            Command::StartTurn => {
                println!("Start turn!");
                //trebuie adaugat: cum decidem unde impuscam

                let args: StartTurnArgs = serde_json::from_value(message.args.clone()).unwrap();
                let mut g=0;
                let mut g1=0;
                let mut g2=0;
                for hero in args.state.heroes{
                    println!("Hero {}: owner {}, at ({}, {}), HP: {}", hero.id, hero.owner_id, hero.x, hero.y, hero.hp);
                    let owner_id=Some(hero.owner_id);
                    
                    goal.x=align(width/2);
                    goal.y=align(height/2);

                    if owner_id != my_id{
                        if hero.id==2{
                            e1.x=hero.x;
                            e1.y=hero.y;
                            g1=1;
                        }
                        if hero.id==3{
                            e2.x=hero.x;
                            e2.y=hero.y;
                            g2=1;
                        }
                        g=1;
                        saw_enemy=true;
                    }

                    if owner_id == my_id{
                    println!("My hero!");
                    let start = Node {
                        x: hero.x,
                        y: hero.y,
                    };

                    if hero.cooldown !=0{
                        println!("Hero on cooldown..");
                    }

                    if hero.cooldown == 0 && saw_enemy==true{
                        sgoal=shoot(start, &args.state.walls, e1, e2);
                        if sgoal.x!=-1 && sgoal.y!=-1{
                            println!("Hero {} shooting at ({}, {})", hero.id, sgoal.x, sgoal.y);
                            if let Err(e) = send_command(
                                &mut write,
                                WebSocketMessage { command: Command::Shoot, 
                                    args: serde_json::json!({"hero_id": hero.id, "x": sgoal.x, "y": sgoal.y}), }
                            )
                            .await{
                                println!("Failed to shoot: {e}");
                        }}
                    }
                    if saw_enemy ==true{
                        e1d=(e1.x-hero.x+e1.y-hero.y).abs();
                        e2d=(e2.x-hero.x+e2.y-hero.y).abs();
                        if e1d<e2d{
                            goal.x=e1.x;
                            goal.y=e1.y;
                        }
                        else {
                            goal.x=e2.x;
                            goal.y=e2.y;
                    }}
                    if let Some(next) = bfs(start, goal, &args.state.walls, &args.state.projectiles, width, height, my_id) {
                            println!("Hero {} moving to ({}, {})", hero.id, next.x, next.y);
                            
                            if let Err(e) = send_command(
                                &mut write,
                                WebSocketMessage {
                                    command: Command::Move,
                                    args: serde_json::json!({"hero_id": hero.id, "x": next.x, "y": next.y}),
                                },
                            )
                            .await
                            {
                                println!("Failed to move: {e}");
                            }

                        } else {

                                  if let Err(e) = send_command(
                                &mut write,
                                WebSocketMessage {
                                    command: Command::Move,
                                    args: serde_json::json!({"hero_id": hero.id, "x": hero.x, "y": hero.y}),
                                },
                            )
                            .await
                            {
                                println!("Failed to move: {e}");
                            }
                        }
                    }
                }
                if g==0{
                    saw_enemy=false;
                }
                if g1==0{
                    e1.x=-1;
                    e1.y=-1;
                }
                if g2==0{
                    e2.x=-1;
                    e2.y=-1;
                }
       }

            Command::Move => {
                println!("Not now!");
            }

            Command::Shoot => {
                println!("Stop violence!");
            }

            Command::EndMatch => {
                let args: EndMatchArgs = serde_json::from_value(message.args.clone()).unwrap();
                println!("End of the match!\nReason: {}\n Winner: {:?}", args.reason, args.winner);
            }
        }
    }
}
