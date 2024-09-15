#!/bin/bash
flatc --python --gen-all --size-prefixed -o src/python/src src/fbs/*.fbs
