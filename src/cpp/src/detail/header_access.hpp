#pragma once

#include <fcb/header.hpp>

struct Header;

namespace fcb {
namespace detail {

/// Internal gateway to HeaderView's generated FlatBuffers pointer.
///
/// C++ cannot bolt a private member onto an already-defined class from
/// another header, so the public class declares raw() private and friends
/// this struct. Decoders use HeaderAccess::get(); consumers cannot.
struct HeaderAccess {
    static const ::Header* get(const HeaderView& h);
};

}  // namespace detail
}  // namespace fcb
