use async_std::{channel, task};
use async_std::channel::unbounded;
use crate::broker::{BrokerToPeerMessage, PeerKind, PeerToBrokerMessage, Receiver, Sender, SubscribePath};
use crate::rpcmessage::CliId;
use crate::{List, RpcMessage, RpcMessageMetaTags, RpcValue};
use crate::broker::state::State;
use crate::broker::config::BrokerConfig;
use crate::broker::node::{METH_SUBSCRIBE, METH_UNSUBSCRIBE};
use crate::rpcframe::RpcFrame;


struct CallCtx<'a> {
    writer: &'a Sender<PeerToBrokerMessage>,
    reader: &'a Receiver<BrokerToPeerMessage>,
    client_id: CliId,
}

async fn call(path: &str, method: &str, param: Option<RpcValue>, ctx: &CallCtx<'_>) -> RpcValue {
    let msg = RpcMessage::new_request(path, method, param);
    let frame = RpcFrame::from_rpcmessage(msg).expect("valid message");
    println!("request: {}", frame.to_rpcmesage().unwrap());
    ctx.writer.send(PeerToBrokerMessage::FrameReceived { client_id: ctx.client_id, frame }).await.unwrap();
    let retval = loop {
        if let BrokerToPeerMessage::SendMessage(msg) = ctx.reader.recv().await.unwrap() {
            println!("response: {msg}");
            if msg.is_response() {
                match msg.result() {
                    Ok(retval) => {
                        break retval.clone();
                    }
                    Err(err) => {
                        panic!("Invalid response received: {err} - {}", msg);
                    }
                }
            } else {
                // ignore RPC requests which might be issued after subscribe call
                continue;
            }
        } else {
            panic!("Response expected");
        }
    };
    retval
}

#[test]
fn test_broker() {
    let config = BrokerConfig::default();
    let access = config.access.clone();
    let (broker_command_sender, _broker_command_receiver) = unbounded();
    let broker = State::new(access, broker_command_sender);
    let roles = broker.flatten_roles("child-broker").unwrap();
    assert_eq!(roles, vec!["child-broker", "device", "client", "ping", "subscribe", "browse"]);
}

#[async_std::test]
async fn test_broker_loop() {
    let config = BrokerConfig::default();
    let access = config.access.clone();
    let (broker_writer, broker_reader) = channel::unbounded();
    //let parent_broker_peer_config = config.parent_broker.clone();
    let broker_task = task::spawn(crate::broker::broker_loop(broker_reader, access));

    let (peer_writer, peer_reader) = channel::unbounded::<BrokerToPeerMessage>();
    let client_id = 2;

    let call_ctx = CallCtx {
        writer: &broker_writer,
        reader: &peer_reader,
        client_id,
    };

    // login
    broker_writer.send(PeerToBrokerMessage::NewPeer { client_id, sender: peer_writer, peer_kind: PeerKind::Client }).await.unwrap();
    broker_writer.send(PeerToBrokerMessage::GetPassword { client_id, user: "admin".into() }).await.unwrap();
    peer_reader.recv().await.unwrap();
    let register_device = PeerToBrokerMessage::RegisterDevice { client_id, device_id: Some("test-device".into()), mount_point: Default::default() };
    broker_writer.send(register_device).await.unwrap();
    broker_writer.send(PeerToBrokerMessage::SetSubscribePath { client_id, subscribe_path: SubscribePath::CanSubscribe(".app/broker/currentClient".into())}).await.unwrap();

    // device should be mounted as 'shv/dev/test'
    let resp = call("shv/test", "ls", Some("device".into()), &call_ctx).await;
    assert_eq!(resp, RpcValue::from(true));

    // test current client info
    let resp = call(".app/broker/currentClient", "info", None, &call_ctx).await;
    let m = resp.as_map();
    assert_eq!(m.get("clientId").unwrap(), &RpcValue::from(2));
    assert_eq!(m.get("mountPoint").unwrap(), &RpcValue::from("shv/test/device"));
    assert_eq!(m.get("userName").unwrap(), &RpcValue::from("admin"));
    assert_eq!(m.get("subscriptions").unwrap(), &RpcValue::from(List::new()));

    // subscriptions
    let subs_param = crate::Map::from([("paths".to_string(), RpcValue::from("shv/**"))]);
    {
        call(".app/broker/currentClient", METH_SUBSCRIBE, Some(RpcValue::from(subs_param.clone())), &call_ctx).await;
        let resp = call(".app/broker/currentClient", "info", None, &call_ctx).await;
        let subs = resp.as_map().get("subscriptions").unwrap();
        let subs_list = subs.as_list();
        assert_eq!(subs_list.len(), 1);
    }
    {
        call(".app/broker/currentClient", METH_UNSUBSCRIBE, Some(RpcValue::from(subs_param.clone())), &call_ctx).await;
        let resp = call(".app/broker/currentClient", "info", None, &call_ctx).await;
        let subs = resp.as_map().get("subscriptions").unwrap();
        let subs_list = subs.as_list();
        assert_eq!(subs_list.len(), 0);
    }

    broker_task.cancel().await;
}