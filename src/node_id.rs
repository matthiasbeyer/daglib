//
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//

/// A unique identifier for a `Node`
///
/// The `NodeId` should be cheap to clone (for example a UUID or some form of a hash value).
pub trait NodeId: Clone + Eq + PartialEq {
}
