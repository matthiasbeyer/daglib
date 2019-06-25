//
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//

use std::fmt::Debug;

use crate::id::Id;

use futures::future::Future;
use futures::stream::Stream;

/// A "Node" is an object of the (DA)Graph
///
/// In git-speak, this would be an "Object".
///
///
/// # Equality
///
/// A node might be compared to another node. In git, for example, equality would be the equality
/// of the nodes ids, because objects are non-mutable.
///
pub trait Node: Debug + PartialEq + Eq {
    type Id: Id;
    type NodePayload: Debug;
    type Error: Debug;
    type Payload: Future<Item = Self::NodePayload, Error = Self::Error>;

    /// It should be trivial to get the Id of a Node.
    fn id(&self) -> Self::Id;

    /// Fetch the payload of the node.
    fn payload(&self) -> Self::Payload;

    /// Fetch the Ids of the parents of this node
    fn parent_ids(&self) -> Stream<Item = Self::Id, Error = Self::Error>;
}

