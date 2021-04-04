use crate::NodeId;

pub trait Node {
    type Id: NodeId;

    fn id(&self) -> &Self::Id;
    fn parent_ids(&self) -> Vec<Self::Id>;
}

