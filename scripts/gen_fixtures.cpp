// Copyright (C) 2026  namazso <admin@namazso.eu>
// SPDX-License-Identifier: MPL-2.0
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

// One-off generator for the .torrent fixtures in rbtorrent/tests/fixtures/.
// The outputs are checked in; re-run only if the fixtures need to change.
//
// Build & run (using the vendored libtorrent install produced by a
// `--features vendored` build):
//
//   PREFIX=target/debug/build/libctorrent-sys-*/out/libtorrent
//   FLAGS=$(PKG_CONFIG_PATH=$PREFIX/lib64/pkgconfig pkg-config
//           --cflags --libs libtorrent-rasterbar)
//   g++ -std=c++17 scripts/gen_fixtures.cpp -o gen_fixtures $FLAGS
//       -Wl,-rpath,$PREFIX/lib64
//   ./gen_fixtures rbtorrent/tests/fixtures
//
// Each fixture contains the same two files with deterministic contents:
//   fixture/a.bin  40960 bytes (spans 3 pieces of 16 KiB)
//   fixture/b.txt  137 bytes
// Every fixture carries a comment, creator and a fixed creation date;
// v1/v2/hybrid additionally carry two tracker tiers and a web seed
// (parsing-surface coverage). transfer.torrent omits exactly those network
// endpoints, so tests that actually add it to a session (the E2E transfer
// test) never touch the network beyond their explicit localhost peer
// connection.

#include <libtorrent/create_torrent.hpp>

#include <cstdint>
#include <cstdio>
#include <filesystem>
#include <fstream>
#include <stdexcept>
#include <string>
#include <vector>

namespace {

namespace fs = std::filesystem;

// xorshift-style deterministic bytes, independent of any library version.
void write_pattern(fs::path const& path, std::size_t size, std::uint32_t seed)
{
	std::ofstream out(path, std::ios::binary);
	std::uint32_t state = seed;
	std::vector<char> bytes;
	bytes.reserve(size);
	for (std::size_t i = 0; i < size; ++i) {
		state ^= state << 13;
		state ^= state >> 17;
		state ^= state << 5;
		bytes.push_back(static_cast<char>(state & 0xff));
	}
	out.write(bytes.data(), static_cast<std::streamsize>(bytes.size()));
	if (!out) throw std::runtime_error("failed to write " + path.string());
}

void generate(fs::path const& root, fs::path const& out_file,
	lt::create_flags_t const flags, bool const hermetic)
{
	fs::path const content = root / "fixture";
	fs::create_directories(content);
	write_pattern(content / "a.bin", 40960, 0xdecafbad);
	write_pattern(content / "b.txt", 137, 0xb0bafe77);

	std::vector<lt::create_file_entry> files;
	files.emplace_back("fixture/a.bin", 40960);
	files.emplace_back("fixture/b.txt", 137);
	lt::create_torrent t(std::move(files), 16384, flags);
	if (!hermetic) {
		t.add_tracker("http://tracker.example.com/announce", 0);
		t.add_tracker("http://backup.example.com/announce", 1);
		t.add_url_seed("http://seed.example.com/fixture");
	}
	t.set_comment("rbtorrent test fixture");
	t.set_creator("rbtorrent gen_fixtures");
	t.set_creation_date(1234567890);

	lt::error_code ec;
	lt::set_piece_hashes(t, root.string(), ec);
	if (ec) throw std::runtime_error("set_piece_hashes: " + ec.message());

	std::vector<char> const buf = t.generate_buf();
	std::ofstream out(out_file, std::ios::binary);
	out.write(buf.data(), static_cast<std::streamsize>(buf.size()));
	if (!out) throw std::runtime_error("failed to write " + out_file.string());
	std::printf("wrote %s (%zu bytes)\n", out_file.string().c_str(),
		buf.size());
}

} // anonymous namespace

int main(int argc, char const* argv[])
{
	if (argc != 2) {
		std::fprintf(stderr, "usage: gen_fixtures <output-dir>\n");
		return 1;
	}
	fs::path const out_dir = argv[1];
	fs::create_directories(out_dir);
	// The payload lives next to the .torrent files (fixture/a.bin, b.txt):
	// tests seed directly from the checked-in directory, so generating and
	// hashing in place keeps payload and piece hashes in lockstep.
	fs::path const work = out_dir;

	try {
		generate(work, out_dir / "v1.torrent", lt::create_torrent::v1_only,
			false);
		generate(work, out_dir / "v2.torrent", lt::create_torrent::v2_only,
			false);
		generate(work, out_dir / "hybrid.torrent", {}, false);
		// Trackerless, web-seedless v1 fixture for tests that add the
		// torrent to a live session and must stay network-hermetic.
		generate(work, out_dir / "transfer.torrent",
			lt::create_torrent::v1_only, true);
	} catch (std::exception const& e) {
		std::fprintf(stderr, "error: %s\n", e.what());
		return 1;
	}
	return 0;
}
