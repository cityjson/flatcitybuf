use crate::error::{Error, Result};
use crate::header_generated::*;
use crate::{check_magic_bytes, HEADER_MAX_BUFFER_SIZE};
use std::io::{self, Read, Seek, SeekFrom};
use std::marker::PhantomData;

pub struct FcbReader<R> {
    reader: R,
    verify: bool,
    // fbs: FcbFeature,
}

pub struct FeatureIter<R, S> {
    reader: R,
    seekable_marker: PhantomData<S>,
}

#[derive(Debug, PartialEq, Eq)]
enum State {
    Init,
    ReadFirstFeatureSize,
    Reading,
    Finished,
}

#[doc(hidden)]
pub mod reader_trait {
    pub struct Seekable;
    pub struct NotSeekable;
}
use reader_trait::*;

impl<R: Read> FcbReader<R> {
    pub fn open(reader: R) -> Result<FcbReader<R>> {
        Self::read_header(reader, true)
    }

    pub unsafe fn open_unchecked(reader: R) -> Result<FcbReader<R>> {
        Self::read_header(reader, false)
    }

    fn read_header(mut reader: R, verify: bool) -> Result<FcbReader<R>> {
        let mut magic_buf: [u8; 8] = [0; 8];
        reader.read_exact(&mut magic_buf)?;
        if !check_magic_bytes(&magic_buf) {
            return Err(Error::MissingMagicBytes);
        }

        let mut size_buf: [u8; 4] = [0; 4];
        reader.read_exact(&mut size_buf)?;
        let header_size = u32::from_le_bytes(size_buf) as usize;
        if header_size > HEADER_MAX_BUFFER_SIZE || header_size < 8 {
            return Err(Error::IllegalHeaderSize(header_size));
        }

        let mut header_buf = Vec::with_capacity(header_size as usize + 4);
        header_buf.extend_from_slice(&size_buf);
        header_buf.resize(header_buf.capacity(), 0);
        reader.read_exact(&mut header_buf[4..])?;

        if verify {
            let _header = size_prefixed_root_as_header(&header_buf);
        }

        Ok(FcbReader { reader, verify })
    }

    //   pub fn select_all_seq(mut self) -> Result<FeatureIter<R, NotSeekable>> {
    //     // skip index
    //     let index_size = self.index_size();
    //     io::copy(&mut (&mut self.reader).take(index_size), &mut io::sink())?;

    //     Ok(FeatureIter::new(self.reader, self.verify, self.fbs, None))
    // }
    //   pub fn select_bbox_seq(
    //     mut self,
    //     min_x: f64,
    //     min_y: f64,
    //     max_x: f64,
    //     max_y: f64,
    // ) -> Result<FeatureIter<R, NotSeekable>> {
    //     // Read R-Tree index and build filter for features within bbox
    //     let header = self.fbs.header();
    //     if header.index_node_size() == 0 || header.features_count() == 0 {
    //         return Err(Error::NoIndex);
    //     }
    //     let index = PackedRTree::from_buf(
    //         &mut self.reader,
    //         header.features_count() as usize,
    //         header.index_node_size(),
    //     )?;
    //     let list = index.search(min_x, min_y, max_x, max_y)?;
    //     debug_assert!(
    //         list.windows(2).all(|w| w[0].offset < w[1].offset),
    //         "Since the tree is traversed breadth first, list should be sorted by construction."
    //     );

    //     Ok(FeatureIter::new(
    //         self.reader,
    //         self.verify,
    //         self.fbs,
    //         Some(list),
    //     ))
    // }
}

impl<R: Read + Seek> FcbReader<R> {
    // pub fn select_all(mut self) -> Result<FeatureIter<R, Seekable>> {
    //   // skip index
    //   // let index_size = self.index_size();
    //   let index_size = 0.0;
    //   self.reader.seek(SeekFrom::Current(index_size as i64))?;

    //   Ok(FeatureIter::new(self.reader, self.verify, self.fbs, None))
    // }

    //   pub fn select_bbox(
    //     mut self,
    //     min_x: f64,
    //     min_y: f64,
    //     max_x: f64,
    //     max_y: f64,
    // ) -> Result<FeatureIter<R, Seekable>> {
    //     // Read R-Tree index and build filter for features within bbox
    //     let header = self.fbs.header();
    //     if header.index_node_size() == 0 || header.features_count() == 0 {
    //         return Err(Error::NoIndex);
    //     }
    //     let list = PackedRTree::stream_search(
    //         &mut self.reader,
    //         header.features_count() as usize,
    //         PackedRTree::DEFAULT_NODE_SIZE,
    //         min_x,
    //         min_y,
    //         max_x,
    //         max_y,
    //     )?;
    //     debug_assert!(
    //         list.windows(2).all(|w| w[0].offset < w[1].offset),
    //         "Since the tree is traversed breadth first, list should be sorted by construction."
    //     );

    //     Ok(FeatureIter::new(
    //         self.reader,
    //         self.verify,
    //         self.fbs,
    //         Some(list),
    //     ))
    // }
}

impl<R: Read> FcbReader<R> {
    // pub fn header(&self) -> Header {
    //   self.fbs.header()
    // }

    // fn index_size(&self) -> u64{
    //   let header = self.fbs.header();
    //   let feat_count = header.features_count() as usize;
    //   if header.index_node_size() > 0 && feat_count > 0{
    //     0
    //   } else {
    //     0
    //   }
    // }
}

// impl<R: Read + Seek> FallibleStreamingIterator for FeatureIter<R, Seekable> {
//   type Item = FgbFeature;
//   type Error = Error;

//   fn advance(&mut self) -> Result<()> {
//       if self.advance_finished() {
//           return Ok(());
//       }
//       if let Some(filter) = &self.item_filter {
//           let item = &filter[self.feat_no];
//           if item.offset as u64 > self.cur_pos {
//               if self.state == State::ReadFirstFeatureSize {
//                   self.state = State::Reading;
//               }
//               // skip features
//               let seek_bytes = item.offset as u64 - self.cur_pos;
//               self.reader.seek(SeekFrom::Current(seek_bytes as i64))?;
//               self.cur_pos += seek_bytes;
//           }
//       }
//       self.read_feature()
//   }

//   fn get(&self) -> Option<&FgbFeature> {
//       self.iter_get()
//   }

//   fn size_hint(&self) -> (usize, Option<usize>) {
//       self.iter_size_hint()
//   }
// }
