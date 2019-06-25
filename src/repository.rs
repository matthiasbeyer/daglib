//
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//

use std::fmt::Debug;

use crate::node;
use crate::id;

use futures::future::Future;

///
pub trait Repository: Debug + Sync + Send {
    type Id: id::Id;
    type Error: Debug;
    type Node: node::Node<Id = Self::Id, Error = Self::Error>;

    /// It should be trivial to get the Id of a Node.
    fn get(&self, id: Self::Id) -> Box<Future<Item = Self::Node, Error = Self::Error>>;
}

