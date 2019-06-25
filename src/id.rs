//
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//

use std::fmt::Debug;

/// An "ID" is an object that can be used to uniquely identify a node in the DAG.
///
/// In git-speak, this would be a SHA1 hash.
///
pub trait Id : Clone + Debug + PartialEq + Eq {

    /* empty */

}

