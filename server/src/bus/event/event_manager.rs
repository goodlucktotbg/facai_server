use kameo::Actor;
use kameo::actor::ActorRef;
use kameo::mailbox::unbounded;
use kameo_actors::DeliveryStrategy;
use kameo_actors::message_bus::{MessageBus};

pub type EventManagerType = MessageBus;

const EVENT_BUS_NAME: &'static str = "event_bus";

pub struct EventManager;

impl EventManager {
    pub async fn actor_ref() -> anyhow::Result<Option<ActorRef<MessageBus>>> {
        let ret = kameo::actor::ActorRef::lookup(EVENT_BUS_NAME)?;
        Ok(ret)
    }

    pub async fn spawn_link(supervisor: &ActorRef<impl Actor>) -> anyhow::Result<()> {
        let message_bus = MessageBus::new(DeliveryStrategy::BestEffort);
        let prepared_bus = Actor::prepare_with_mailbox(unbounded::<MessageBus>());
        prepared_bus.actor_ref().register(EVENT_BUS_NAME)?;
        prepared_bus.actor_ref().link(supervisor).await;
        prepared_bus.spawn(message_bus);

        Ok(())
    }
}
