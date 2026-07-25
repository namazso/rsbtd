// Copyright (C) 2026  namazso <admin@namazso.eu>
// SPDX-License-Identifier: MPL-2.0
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! [`ClientData`]: opaque per-torrent client state, persisted inside
//! resume data.

/// Per-torrent client state attached at [`Session::add_torrent`] time and
/// carried in resume data under the top-level `"rbt-data"` bencode key.
///
/// The crate never inspects the concrete type: it stores the value as
/// `Arc<dyn ClientData>` for the torrent's lifetime, serializes it when
/// resume data is written ([`TorrentHandle::write_resume_data`],
/// [`SaveResumeDataAlert::write_resume_data`]), and hands the raw bytes
/// back on [`Session::read_resume_data_with`]. Implementations must uphold
/// the round-trip law: `from_bencode(Some(&x.to_bencode()))` reconstructs
/// `x`.
///
/// `to_bencode` returning an empty vector means "write no key"; a missing
/// key on read yields `from_bencode(None)`, from which implementations
/// should produce their defaults — this is what makes resume data written
/// before the data type existed load cleanly. The two cannot collide:
/// valid bencode is never zero bytes.
///
/// `to_bencode` runs on the alert-processing path — keep it cheap.
///
/// [`Session::add_torrent`]: crate::Session::add_torrent
/// [`Session::read_resume_data_with`]: crate::Session::read_resume_data_with
/// [`TorrentHandle::write_resume_data`]: crate::TorrentHandle::write_resume_data
/// [`SaveResumeDataAlert::write_resume_data`]: crate::alerts::SaveResumeDataAlert::write_resume_data
pub trait ClientData: std::any::Any + Send + Sync {
    /// Serializes the value to exactly one well-formed bencode value: the
    /// bytes are spliced verbatim into the resume data as the `"rbt-data"`
    /// key's value (the writers validate this and fail on a malformed
    /// blob rather than emit a corrupt file). Empty means "no data": no
    /// key is written.
    fn to_bencode(&self) -> Vec<u8>;

    /// Reconstructs a value from the bytes previously produced by
    /// [`to_bencode`](ClientData::to_bencode), or from `None` when the
    /// resume data carries no `"rbt-data"` key (defaults). Errors are
    /// typically built with [`Error::client`](crate::Error::client).
    fn from_bencode(bytes: Option<&[u8]>) -> crate::Result<Self>
    where
        Self: Sized;
}

/// The null client data: carries nothing, writes no key, accepts anything.
impl ClientData for () {
    fn to_bencode(&self) -> Vec<u8> {
        Vec::new()
    }

    fn from_bencode(_bytes: Option<&[u8]>) -> crate::Result<()> {
        Ok(())
    }
}
