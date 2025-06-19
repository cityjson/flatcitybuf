# Attribution

This document provides attribution for third-party code and libraries used in FlatCityBuf.

## FlatGeobuf

**Portions of this software are derived from FlatGeobuf**

- **Project**: FlatGeobuf
- **Source**: <https://github.com/flatgeobuf/flatgeobuf>
- **License**: BSD 2-Clause License
- **Copyright**: (c) 2018-2024, Björn Harrtell and contributors

### Derived Components

The following components in FlatCityBuf contain code derived from or inspired by FlatGeobuf:

1. **Spatial Indexing (`packed_rtree` module)**

   - Packed R-tree implementation for spatial queries
   - Hilbert curve ordering for spatial clustering
   - Bounding box calculations and spatial predicates

2. **HTTP Range Request Handling (`http_reader` module)**

   - HTTP range request patterns for streaming data
   - Efficient partial file reading over HTTP
   - Connection pooling and retry logic

3. **FlatBuffers Integration Patterns**

   - Binary format structure and layout
   - Zero-copy deserialization patterns
   - Buffer alignment and padding strategies

4. **Binary Format Design**
   - Magic bytes and version handling
   - Header structure and metadata organization
   - Feature indexing and offset management

### FlatGeobuf License (BSD 2-Clause)

```
BSD 2-Clause License

Copyright (c) 2018-2024, Björn Harrtell and contributors
All rights reserved.

Redistribution and use in source and binary forms, with or without
modification, are permitted provided that the following conditions are met:

1. Redistributions of source code must retain the above copyright notice, this
   list of conditions and the following disclaimer.

2. Redistributions in binary form must reproduce the above copyright notice,
   this list of conditions and the following disclaimer in the documentation
   and/or other materials provided with the distribution.

THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS"
AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE
IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE
FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL
DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER
CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY,
OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE
OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.
```

## Acknowledgments

We extend our sincere gratitude to:

- **Björn Harrtell** and the **FlatGeobuf team** for creating an excellent foundation for efficient geospatial binary formats
- The **FlatBuffers team at Google** for the cross-platform serialization library
- The **CityJSON community** for defining the semantic standards for 3D city models
- The **Rust community** for providing excellent tools and libraries for systems programming

## License Compatibility

- **FlatCityBuf**: MIT License
- **FlatGeobuf derived code**: BSD 2-Clause License (compatible with MIT)
- **Combined work**: Distributed under MIT License, with BSD 2-Clause components clearly attributed

The BSD 2-Clause License is compatible with the MIT License, allowing for the combination of both licensed works under the MIT License while maintaining proper attribution to the original BSD-licensed components.
