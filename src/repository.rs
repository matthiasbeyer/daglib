//
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//

use std::fmt::Debug;

use crate::node::Node;
use crate::id::Id;

use futures::future::Future;

///
pub trait Repository: Debug {
    type Id: Id;
    type Error: Debug;
    type Node: Node<Id = Self::Id, Error = Self::Error>;

    type Get: Future<Item = Self::Node, Error = Self::Error>;

    /// It should be trivial to get the Id of a Node.
    fn get<ID>(id: ID) -> Result<Self::Get, Self::Error>
        where ID: Id;

}

