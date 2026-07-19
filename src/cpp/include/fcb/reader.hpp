#pragma once

#include <cstdint>
#include <memory>
#include <string>
#include <vector>

#include <fcb/error.hpp>
#include <fcb/feature.hpp>
#include <fcb/header.hpp>
#include <fcb/range_reader.hpp>

namespace fcb {

/// One hit from an index traversal.
struct SearchResultItem {
    std::uint64_t offset;  // relative to the features section
    std::uint64_t index;   // feature ordinal
};

/// How a FeatureIterator decides what to visit.
///
/// This is an explicit mode rather than "an empty offset list means scan
/// everything": an empty list is also the perfectly normal result of a
/// query that matched nothing, and conflating the two would silently turn
/// a zero-result query into a full scan.
enum class IterationMode {
    SequentialScan,  ///< walk the features section start to finish
    OffsetList,      ///< visit exactly the offsets supplied (possibly none)
};

/// Single-pass iterator over features. Not copyable.
class FeatureIterator {
public:
    FeatureIterator(std::shared_ptr<RangeReader> reader,
                    HeaderView header,
                    IterationMode mode,
                    std::vector<SearchResultItem> hits);

    FeatureIterator(const FeatureIterator&) = delete;
    FeatureIterator& operator=(const FeatureIterator&) = delete;
    FeatureIterator(FeatureIterator&&) = default;
    FeatureIterator& operator=(FeatureIterator&&) = delete;

    /// Advance. Returns false once iteration is complete.
    /// Throws if the file is truncated before features_count features.
    bool next();

    const Feature& current() const { return current_; }

    /// Total features the header claims, for progress reporting.
    std::uint64_t features_count() const { return header_.info().features_count; }

private:
    std::shared_ptr<RangeReader> reader_;
    HeaderView header_;
    IterationMode mode_;
    std::vector<SearchResultItem> hits_;

    Feature current_;
    std::uint64_t cursor_ = 0;    // absolute, for SequentialScan
    std::size_t hit_index_ = 0;   // for OffsetList
    std::uint64_t produced_ = 0;
};

/// The library's entry point.
class FcbReader {
public:
    static FcbReader open_file(const std::string& path);
    static FcbReader open(std::shared_ptr<RangeReader> reader);

    const HeaderView& header() const { return header_; }

    /// Iterate every feature in stored (Hilbert) order.
    FeatureIterator select_all();

private:
    FcbReader(std::shared_ptr<RangeReader> reader, HeaderView header);

    std::shared_ptr<RangeReader> reader_;
    HeaderView header_;
};

}  // namespace fcb
