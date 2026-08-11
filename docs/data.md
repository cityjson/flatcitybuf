# Datasets

Two public hosts, both backed by Cloudflare R2 and both serving HTTP range
requests (`206 Partial Content` with `Accept-Ranges: bytes`), so a reader
fetches only the bytes a query needs instead of the whole file:

| Host | Contents |
|---|---|
| `https://flatcitybuf.open3d.city/data/` | FlatCityBuf `.fcb` |
| `https://cityjson.open3d.city/cityjsonseq/` | CityJSONSeq `.jsonl` |

A `.fcb` URL can be handed straight to a reader — `fcb inspect <url>`, the
Python `HttpRangeReader`, `FcbReader.fromUrl` in TypeScript, `fcb_read_http` in
C++, or the URL box in the [web viewer](../examples/web/README.md). For a file
of national scale that is the whole point: opening `3dbag_all_index.fcb`
(68.5 GB) costs two requests, and a 1 km bounding-box query a few dozen more.

Sizes below are decimal (1 MB = 10⁶ bytes, 1 GB = 10⁹ bytes).

## FlatCityBuf (`.fcb`)

| Name | Size | URL |
|---|---|---|
| `3DBAG.city.fcb` | 7.1 MB | https://flatcitybuf.open3d.city/data/3DBAG.city.fcb |
| `3DBV.city.fcb` | 321.0 MB | https://flatcitybuf.open3d.city/data/3DBV.city.fcb |
| `3dbag_subset.city.fcb` | 2.32 GB | https://flatcitybuf.open3d.city/data/3dbag_subset.city.fcb |
| `Helsinki.city.fcb` | 388.1 MB | https://flatcitybuf.open3d.city/data/Helsinki.city.fcb |
| `Helsinki_tex.city.fcb` | 598.6 MB | https://flatcitybuf.open3d.city/data/Helsinki_tex.city.fcb |
| `Ingolstadt.city.fcb` | 3.3 MB | https://flatcitybuf.open3d.city/data/Ingolstadt.city.fcb |
| `Montreal.city.fcb` | 5.0 MB | https://flatcitybuf.open3d.city/data/Montreal.city.fcb |
| `NYC.fcb` | 85.0 MB | https://flatcitybuf.open3d.city/data/NYC.fcb |
| `Railway.city.fcb` | 3.9 MB | https://flatcitybuf.open3d.city/data/Railway.city.fcb |
| `Rotterdam.fcb` | 3.0 MB | https://flatcitybuf.open3d.city/data/Rotterdam.fcb |
| `Vienna.city.fcb` | 4.4 MB | https://flatcitybuf.open3d.city/data/Vienna.city.fcb |
| `Zurich.city.fcb` | 206.3 MB | https://flatcitybuf.open3d.city/data/Zurich.city.fcb |
| `plateau_takeshiba_bldg.city.fcb` | 83.8 MB | https://flatcitybuf.open3d.city/data/plateau_takeshiba_bldg.city.fcb |
| `plateau_takeshiba_brid.city.fcb` | 5.5 MB | https://flatcitybuf.open3d.city/data/plateau_takeshiba_brid.city.fcb |
| `plateau_takeshiba_rwy.city.fcb` | 4.5 MB | https://flatcitybuf.open3d.city/data/plateau_takeshiba_rwy.city.fcb |
| `plateau_takeshiba_tran.city.fcb` | 28.4 MB | https://flatcitybuf.open3d.city/data/plateau_takeshiba_tran.city.fcb |
| `plateau_takeshiba_tun.city.fcb` | 4.9 MB | https://flatcitybuf.open3d.city/data/plateau_takeshiba_tun.city.fcb |
| `plateau_takeshiba_veg.city.fcb` | 2.5 MB | https://flatcitybuf.open3d.city/data/plateau_takeshiba_veg.city.fcb |
| `tokyo_plateau.city.fcb` | 232.1 MB | https://flatcitybuf.open3d.city/data/tokyo_plateau.city.fcb |
| `3dbag_all_index.fcb` | 68.55 GB | https://flatcitybuf.open3d.city/data/3dbag_all_index.fcb |
| `3dbag_subset_all_index.fcb` | 3.81 GB | https://flatcitybuf.open3d.city/data/3dbag_subset_all_index.fcb |
| `3dbag_subset2_all_index.fcb` | 7.59 GB | https://flatcitybuf.open3d.city/data/3dbag_subset2_all_index.fcb |

The three `*_all_index.fcb` files at the bottom are the benchmark set: whole
3DBAG and two subsets, serialised with **all attributes indexed** (`fcb ser -A`,
branching factor 256). `3dbag_all_index.fcb` is what the opt-in remote suite
(`just test-remote`, and `FCB_REMOTE_HTTP_URL` in every language's justfile),
the `read_http` benchmark, the `fcb_api` server default and the web viewer's
default URL all point at. The rest of the table is the demonstration corpus,
one file per source dataset.

## CityJSONSeq (`.jsonl`)

| Name | Size | URL |
|---|---|---|
| `3DBAG.city.jsonl` | 6.2 MB | https://cityjson.open3d.city/cityjsonseq/3DBAG.city.jsonl |
| `3DBV.city.jsonl` | 332.8 MB | https://cityjson.open3d.city/cityjsonseq/3DBV.city.jsonl |
| `3dbag_subset.city.jsonl` | 3.03 GB | https://cityjson.open3d.city/cityjsonseq/3dbag_subset.city.jsonl |
| `Helsinki.city.jsonl` | 432.5 MB | https://cityjson.open3d.city/cityjsonseq/Helsinki.city.jsonl |
| `Helsinki_tex.city.jsonl` | 675.0 MB | https://cityjson.open3d.city/cityjsonseq/Helsinki_tex.city.jsonl |
| `Ingolstadt.city.jsonl` | 4.0 MB | https://cityjson.open3d.city/cityjsonseq/Ingolstadt.city.jsonl |
| `Montreal.city.jsonl` | 4.8 MB | https://cityjson.open3d.city/cityjsonseq/Montreal.city.jsonl |
| `NYC.jsonl` | 100.1 MB | https://cityjson.open3d.city/cityjsonseq/NYC.jsonl |
| `Railway.city.jsonl` | 4.2 MB | https://cityjson.open3d.city/cityjsonseq/Railway.city.jsonl |
| `Rotterdam.jsonl` | 2.8 MB | https://cityjson.open3d.city/cityjsonseq/Rotterdam.jsonl |
| `Vienna.city.jsonl` | 5.0 MB | https://cityjson.open3d.city/cityjsonseq/Vienna.city.jsonl |
| `Zurich.city.jsonl` | 259.1 MB | https://cityjson.open3d.city/cityjsonseq/Zurich.city.jsonl |
| `plateau_takeshiba_bldg.city.jsonl` | 80.7 MB | https://cityjson.open3d.city/cityjsonseq/plateau_takeshiba_bldg.city.jsonl |
| `plateau_takeshiba_brid.city.jsonl` | 5.0 MB | https://cityjson.open3d.city/cityjsonseq/plateau_takeshiba_brid.city.jsonl |
| `plateau_takeshiba_rwy.city.jsonl` | 4.4 MB | https://cityjson.open3d.city/cityjsonseq/plateau_takeshiba_rwy.city.jsonl |
| `plateau_takeshiba_tran.city.jsonl` | 27.8 MB | https://cityjson.open3d.city/cityjsonseq/plateau_takeshiba_tran.city.jsonl |
| `plateau_takeshiba_tun.city.jsonl` | 5.1 MB | https://cityjson.open3d.city/cityjsonseq/plateau_takeshiba_tun.city.jsonl |
| `plateau_takeshiba_veg.city.jsonl` | 1.9 MB | https://cityjson.open3d.city/cityjsonseq/plateau_takeshiba_veg.city.jsonl |
| `tokyo_plateau.city.jsonl` | 219.8 MB | https://cityjson.open3d.city/cityjsonseq/tokyo_plateau.city.jsonl |

Where the stems match, the two tables are the same source dataset in the two
formats — `Zurich.city.jsonl` is the input `Zurich.city.fcb` was serialised
from, and so on. Watch the naming: most files carry a `.city` infix, but
`NYC` and `Rotterdam` do not (`NYC.fcb`/`NYC.jsonl`,
`Rotterdam.fcb`/`Rotterdam.jsonl`).

## Notes

- **Range requests.** Verify a host with
  `curl -sI -r 0-0 https://flatcitybuf.open3d.city/data/3DBAG.city.fcb` —
  expect `HTTP/2 206`, `accept-ranges: bytes` and a `content-range` whose total
  is the size above.
- **CORS.** A browser client additionally needs `Content-Range` (and
  `Accept-Ranges`) listed in the bucket's `ExposeHeaders`; without it the reader
  cannot learn the file size and refuses to guess. See
  [Testing §7.3](TESTING.md) and the
  [web viewer troubleshooting](../examples/web/README.md#troubleshooting).
- **Local fixtures.** The small files the test suites use are in the repository,
  not here: `examples/data/delft.fcb` and the conformance corpus under
  `conformance/`.
- These hosts replaced the project's former Google Cloud Storage bucket; the
  blobs are unchanged, only the URLs moved.
