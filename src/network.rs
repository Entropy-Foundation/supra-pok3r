use futures::future::Either;
use libp2p::{
    core::{muxing::StreamMuxerBox, transport::OrTransport, upgrade},
    gossipsub,
    identity::{self},
    noise,
    swarm::{NetworkBehaviour, SwarmEvent},
    tcp, yamux, PeerId, SwarmBuilder, Transport,
};
use libp2p_quic as quic;
use std::collections::{hash_map::DefaultHasher, HashMap, HashSet};
use std::error::Error;
use std::hash::{Hash, Hasher};
use std::time::Duration;

use futures::StreamExt;
use tokio::sync::mpsc;
use tokio::{select, sync::mpsc::unbounded_channel};

use crate::{
    address_book::{get_node_id_via_peer_id, InstanceId, Pok3rAddrBook, Pok3rPeerId},
    common::EvalNetMsg,
};

#[cfg(feature = "mdns")]
use libp2p::mdns;
#[cfg(not(feature = "mdns"))]
use libp2p::Multiaddr;

// We create a custom network behaviour that combines Gossipsub and Mdns.
#[derive(NetworkBehaviour)]
struct MyBehaviour {
    gossipsub: gossipsub::Behaviour,
    #[cfg(feature = "mdns")]
    mdns: mdns::tokio::Behaviour,
}

pub async fn run_networking_daemon(
    id_keys: identity::Keypair,
    addr_book: &Pok3rAddrBook,
    tx: &mut mpsc::UnboundedSender<EvalNetMsg>,
    rx: mpsc::UnboundedReceiver<EvalNetMsg>,
) -> Result<(), Box<dyn Error>> {
    let (_tx, _rx) = unbounded_channel();
    run_networking_daemon_with_kill(id_keys, addr_book, tx, rx, _rx).await
}

pub async fn run_networking_daemon_with_kill(
    id_keys: identity::Keypair,
    addr_book: &Pok3rAddrBook,
    tx: &mut mpsc::UnboundedSender<EvalNetMsg>,
    mut rx: mpsc::UnboundedReceiver<EvalNetMsg>,
    mut rx_kill: mpsc::UnboundedReceiver<()>,
) -> Result<(), Box<dyn Error>> {
    // Create a random PeerId from secret key
    let local_peer_id = PeerId::from(id_keys.public());
    #[cfg(feature = "print")]
    println!("Local peer id: {local_peer_id}");

    // Set up an encrypted DNS-enabled TCP Transport over the yamux protocol.
    let tcp_transport = tcp::tokio::Transport::new(tcp::Config::default().nodelay(true))
        .upgrade(upgrade::Version::V1Lazy)
        .authenticate(noise::Config::new(&id_keys).expect("signing libp2p-noise static keypair"))
        .multiplex(yamux::Config::default())
        .timeout(Duration::from_secs(20))
        .boxed();
    let quic_transport = quic::tokio::Transport::new(quic::Config::new(&id_keys));
    let transport = OrTransport::new(quic_transport, tcp_transport)
        .map(|either_output, _| match either_output {
            Either::Left((peer_id, muxer)) => (peer_id, StreamMuxerBox::new(muxer)),
            Either::Right((peer_id, muxer)) => (peer_id, StreamMuxerBox::new(muxer)),
        })
        .boxed();

    // To content-address message, we can take the hash of message and use it as an ID.
    let message_id_fn = |message: &gossipsub::Message| {
        let mut s = DefaultHasher::new();
        message.data.hash(&mut s);
        gossipsub::MessageId::from(s.finish().to_string())
    };

    // Set a custom gossipsub configuration
    let gossipsub_config = gossipsub::ConfigBuilder::default()
        .heartbeat_interval(Duration::from_secs(10)) // This is set to aid debugging by not cluttering the log space
        .validation_mode(gossipsub::ValidationMode::Strict) // This sets the kind of message validation. The default is Strict (enforce message signing)
        .message_id_fn(message_id_fn) // content-address messages. No two messages of the same content will be propagated.
        .build()
        .expect("Valid config");

    // build a gossipsub network behaviour
    let mut gossipsub = gossipsub::Behaviour::new(
        gossipsub::MessageAuthenticity::Signed(id_keys.clone()),
        gossipsub_config,
    )
    .expect("Correct configuration");
    // Create a Gossipsub topic
    let topic = gossipsub::IdentTopic::new("mpc-test-net");
    // subscribes to our topic
    gossipsub.subscribe(&topic)?;

    // Create a Swarm to manage peers and events
    #[cfg(feature = "mdns")]
    let mdns = mdns::tokio::Behaviour::new(mdns::Config::default(), local_peer_id)?;
    let behaviour = MyBehaviour {
        gossipsub,
        #[cfg(feature = "mdns")]
        mdns,
    };
    let mut swarm: libp2p::Swarm<_> = SwarmBuilder::with_new_identity()
        .with_tokio()
        .with_other_transport(|_| transport)?
        .with_behaviour(|_| behaviour)?
        .build();

    // Read full lines from stdin
    //let mut stdin = io::BufReader::new(io::stdin()).lines().fuse();

    // Listen on all interfaces and whatever port the OS assigns
    #[cfg(feature = "mdns")]
    swarm.listen_on("/ip4/0.0.0.0/udp/0/quic-v1".parse()?)?;
    #[cfg(not(feature = "mdns"))]
    let allowed = {
        let local_peer_b58 = local_peer_id.to_base58();
        let me = addr_book
            .get(&local_peer_b58)
            .expect("addr_book must contain this node's peer id");

        // listen on *my* port
        swarm.listen_on(format!("/ip4/0.0.0.0/udp/{}/quic-v1", me.port).parse()?)?;
        // --- Allowlist from addr_book -------------------------------------------
        let mut allowed: HashSet<PeerId> = HashSet::new();
        for pid_b58 in addr_book.keys() {
            let pid: PeerId = pid_b58.parse()?; // base58 -> PeerId
            allowed.insert(pid);
            // also tell gossipsub we want to treat them as explicit peers
            swarm.behaviour_mut().gossipsub.add_explicit_peer(&pid);
        }

        let local_peer_b58 = local_peer_id.to_base58();

        // --- Dial peers from addr_book (private IPs) -----------------------------
        for (pid_b58, peer) in addr_book.iter() {
            if *pid_b58 == local_peer_b58 {
                continue; // don't dial self
            }
            let ma: Multiaddr = format!("/ip4/{}/udp/{}/quic-v1", peer.ip, peer.port).parse()?;
            if let Err(e) = swarm.dial(ma) {
                eprintln!("dial {} failed: {:?}", pid_b58, e);
            }
        }
        allowed
    };

    let mut connected_peers: HashSet<PeerId> = HashSet::new();
    let mut connection_informed: bool = false;
    #[cfg(not(feature = "mdns"))]
    let target_count = addr_book.len().saturating_sub(1); // everyone except me

    loop {
        select! {
            _ = rx_kill.recv() => {
                // optionally print based on what is received
                break;
            }
            //receives requests for publishing messages from the evaluator
            msg_to_send = rx.recv() => {
                let s = serde_json::to_string(&msg_to_send).unwrap();
                if let Err(e) = swarm
                    .behaviour_mut().gossipsub
                    .publish(topic.clone(), <String as AsRef<[u8]>>::as_ref(&s)) {
                    println!("Publish error: {e:?}");
                }
            },
            //discovers peers, and notifies evaluator when all peers in addr_book are connected
            event = swarm.select_next_some() => match event {
                #[cfg(feature = "mdns")]
                SwarmEvent::Behaviour(MyBehaviourEvent::Mdns(mdns::Event::Discovered(list))) => {
                    for (peer_id, _multiaddr) in list {
                        #[cfg(feature = "print")]
                        println!("mDNS discovered a new peer: {peer_id}");
                        let peer_id_encoded = peer_id.to_base58();
                        swarm.behaviour_mut().gossipsub.add_explicit_peer(&peer_id);

                        if addr_book.contains_key(&peer_id_encoded) {
                            connected_peers.insert(peer_id);

                            if !connection_informed &&
                                (connected_peers.len() == addr_book.len() - 1) {
                                let _r = tx.send(
                                    EvalNetMsg::ConnectionEstablished { success: true }
                                );
                                // if let Err(err) = r {
                                //     eprint!("network error {:?}", err);
                                // }
                                connection_informed = true;
                            }
                        }
                    }
                },
                //handle peers that have dropped off unexpectedly
                #[cfg(feature = "mdns")]
                SwarmEvent::Behaviour(MyBehaviourEvent::Mdns(mdns::Event::Expired(list))) => {
                    for (peer_id, _multiaddr) in list {
                        println!("mDNS discover peer has expired: {peer_id}");
                        swarm.behaviour_mut().gossipsub.remove_explicit_peer(&peer_id);
                    }
                },
                //all received messages over gossip channel are pushed to the evaluator
                SwarmEvent::Behaviour(MyBehaviourEvent::Gossipsub(gossipsub::Event::Message {
                    propagation_source: _peer_id,
                    message_id: _id,
                    message,
                })) => {
                    let msg_as_str = String::from_utf8_lossy(&message.data);
                    let deserialized_struct = serde_json::from_str(&msg_as_str).unwrap();
                    let r = tx.send(deserialized_struct);
                    if let Err(err) = r {
                        eprint!("network error {:?}", err);
                    }
                },
                #[cfg(not(feature = "mdns"))]
                SwarmEvent::ConnectionEstablished { peer_id, .. } => {
                    // Enforce allowlist
                    if !allowed.contains(&peer_id) {
                        let _ = swarm.disconnect_peer_id(peer_id);
                        continue;
                    }
                    if connected_peers.insert(peer_id) && !connection_informed {
                        if connected_peers.len() == target_count {
                            let _ = tx.send(EvalNetMsg::ConnectionEstablished { success: true });
                            connection_informed = true;
                        }
                    }
                }
                #[cfg(not(feature = "mdns"))]
                SwarmEvent::ConnectionClosed { peer_id, .. } => {
                    connected_peers.remove(&peer_id);
                    // we may want to re-dial here
                }
                //prints out the address this program is listening on for new connections
                #[allow(unused_variables)]
                SwarmEvent::NewListenAddr { address, .. } => {
                    #[cfg(feature = "print")]
                    println!("Local node is listening on {address}");
                }
                _ => {}
            }
        }
    }
    Ok(())
}

pub struct MessagingSystem {
    /// local peer id
    pub id: InstanceId, //Pok3rPeerId,
    /// information about all other peers
    pub addr_book: Pok3rAddrBook,
    /// receiver channel from the networkd
    rx: mpsc::UnboundedReceiver<EvalNetMsg>,
    /// sender channel towards the networkd
    tx: mpsc::UnboundedSender<EvalNetMsg>,
    /// stores incoming messages indexed by identifier and then by peer id
    mailbox: HashMap<String, HashMap<InstanceId, String>>,
}

impl MessagingSystem {
    pub async fn new(
        id: &Pok3rPeerId,
        addr_book: Pok3rAddrBook,
        tx: mpsc::UnboundedSender<EvalNetMsg>,
        mut rx: mpsc::UnboundedReceiver<EvalNetMsg>,
    ) -> Self {
        // we expect the first message from the
        // networkd to be a connection established;
        // so, here we will loop till we get that
        loop {
            //do a blocking recv on the rx channel
            let msg: EvalNetMsg = rx.recv().await.expect("failed to get message");
            match msg {
                EvalNetMsg::ConnectionEstablished { success } => {
                    if success {
                        #[cfg(feature = "print")]
                        println!("evaluator connected to the network");
                        break;
                    }
                }
                _ => continue,
            }
        }

        MessagingSystem {
            id: id.clone(),
            addr_book,
            rx,
            tx,
            mailbox: HashMap::new(),
        }
    }

    pub fn get_my_id(&self) -> u64 {
        get_node_id_via_peer_id(&self.addr_book, &self.id).unwrap()
    }

    pub async fn send_to_all(
        &mut self,
        handles: impl AsRef<[String]>,
        values: impl AsRef<[String]>,
    ) {
        assert!(handles.as_ref().len() == values.as_ref().len() && !handles.as_ref().is_empty());

        let msg = if handles.as_ref().len() > 1 {
            EvalNetMsg::PublishBatchValue {
                sender: self.id.clone(),
                handles: handles.as_ref().to_owned(),
                values: values.as_ref().to_owned(),
            }
        } else {
            EvalNetMsg::PublishValue {
                sender: self.id.clone(),
                handle: handles.as_ref()[0].clone(),
                value: values.as_ref()[0].clone(),
            }
        };
        let r = self.tx.send(msg);
        if let Err(err) = r {
            eprint!("evaluator error {:?}", err);
        }
    }

    pub async fn recv_from_all(&mut self, identifier: &String) -> HashMap<u64, String> {
        self.recv_from_all_with_timeout(identifier, 30)
            .await
            .expect("timeout on recv all")
    }
    pub async fn recv_from_all_with_timeout(
        &mut self,
        identifier: &String,
        peer_timeout: u64,
    ) -> Result<HashMap<u64, String>, String> {
        let mut messages: HashMap<u64, String> = HashMap::new();
        let peers: Vec<Pok3rPeerId> = self.addr_book.keys().cloned().collect();
        for peer_id in peers {
            if self.id.eq(&peer_id) {
                continue;
            } // ignore self

            let wait_res =
                tokio::time::timeout(tokio::time::Duration::from_secs(peer_timeout), async {
                    loop {
                        // do we already have this sender's message for this identifier?
                        if let Some(sender_map) = self.mailbox.get(identifier) {
                            if sender_map.contains_key(&peer_id) {
                                break;
                            }
                        }

                        let msg: EvalNetMsg = self.rx.recv().await.expect("failed to get message");
                        self.process_next_message(&msg);
                    }
                })
                .await;

            if wait_res.is_err() {
                return Err(format!("timed out on recv_from_all: {identifier}"));
            }

            let msg = self
                .mailbox
                .get(identifier)
                .unwrap()
                .get(&peer_id)
                .unwrap()
                .clone();
            let peer_id_as_u64 = get_node_id_via_peer_id(&self.addr_book, &peer_id).unwrap();

            messages.insert(peer_id_as_u64, msg);
        }

        //clear the mailbox because we might want to use identifier again
        self.mailbox.remove(identifier);

        Ok(messages)
    }

    fn process_next_message(&mut self, msg: &EvalNetMsg) {
        match msg {
            EvalNetMsg::PublishValue {
                sender,
                handle,
                value,
            } => {
                if self.is_valid_sender(sender) {
                    self.accept_handle_and_value_from_sender(sender, handle, value);
                }
            }
            EvalNetMsg::PublishBatchValue {
                sender,
                handles,
                values,
            } => {
                if self.is_valid_sender(sender) {
                    assert_eq!(handles.len(), values.len());

                    for (h, v) in handles.iter().zip(values.iter()) {
                        self.accept_handle_and_value_from_sender(sender, h, v);
                    }
                }
            }
            _ => (),
        }
    }

    fn accept_handle_and_value_from_sender(
        &mut self,
        sender: &String,
        handle: &String,
        value: &String,
    ) {
        // if already exists, then ignore
        if let Some(mail) = self.mailbox.get(handle) {
            if mail.contains_key(sender) {
                return;
            } //ignore duplicate msg!
        } else {
            //mailbox never got a message by this handle so lets make room for it
            self.mailbox.insert(handle.clone(), HashMap::new());
        }

        self.mailbox
            .get_mut(handle)
            .unwrap()
            .insert(sender.clone(), value.clone());
    }

    fn is_valid_sender(&self, sender_id: &Pok3rPeerId) -> bool {
        // Only accept from known instance IDs
        self.addr_book.contains_key(sender_id)
    }
}
