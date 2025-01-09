# flatcitybuf

Experimental implementation of CityJSON encoding in FlatBuffers.

## Acknowledgements
This project incorporates code from  FlatGeobuf(https://github.com/flatgeobuf/flatgeobuf/tree/master?tab=readme-ov-file), Copyright (c) 2018, Björn Harrtell, licensed under the BSD 2-Clause License.

Also, the FlatBuffers schema for CityBuf feature format is originally authored by TU Delft 3D geoinformation group, Ravi Peters (3DGI), Balazs Dukai (3DGI).

# Run
Serialize CityJSON to CityBuf
```
	cargo run --bin flatcitybuf_cli serialize -i tests/data/delft.city.jsonl -o temp/delft.fcb
```
Deserialize CityBuf to CityJSON
```
	cargo run --bin flatcitybuf_cli deserialize -i temp/delft.fcb -o temp/delft.city.jsonl
```

# References

- https://github.com/cityjson/cityjson-spec
- https://github.com/google/flatbuffers
- https://github.com/Ylannl/CityBuf
- https://github.com/flatgeobuf/flatgeobuf