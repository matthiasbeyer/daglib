//
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//

use crate::NodeId;

/// A Node in the DAG, holding the data.
pub trait Node {
    type Id: NodeId;
    fn parent_ids(&self) -> Vec<Self::Id>;
}

