#[derive(Debug, thiserror::Error)]
pub enum Error<Id>
where
    Id: crate::NodeId,
{
    #[error("Node not found: {:?}", .0)]
    NodeNotFound(Id),

    #[error("Node does not have HEAD as parent")]
    HeadNotAParent,
}
