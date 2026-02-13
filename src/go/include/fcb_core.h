#ifndef FCB_CORE_H
#define FCB_CORE_H

#include <stdarg.h>
#include <stdbool.h>
#include <stdint.h>
#include <stdlib.h>

/**
 * Opaque iterator type exposed to C/Go
 */
typedef struct FcbFileIterator FcbFileIterator;

/**
 * Opaque reader type exposed to C/Go
 */
typedef struct FcbFileReader FcbFileReader;

/**
 * Open an FCB file for reading. Returns null on error.
 * On error, `error_out` is set to an error message (caller must free with `fcb_free_string`).
 */
struct FcbFileReader *fcb_reader_open(const char *path, char **error_out);

/**
 * Get the feature count from an open reader.
 */
uint64_t fcb_reader_features_count(const struct FcbFileReader *reader);

/**
 * Check if the reader has a spatial index.
 */
bool fcb_reader_has_spatial_index(const struct FcbFileReader *reader);

/**
 * Get CityJSON metadata as a JSON string.
 * Caller must free the returned string with `fcb_free_string`.
 */
char *fcb_reader_cityjson_metadata(const struct FcbFileReader *reader, char **error_out);

/**
 * Select all features. Consumes the reader.
 * Returns null on error.
 */
struct FcbFileIterator *fcb_reader_select_all(struct FcbFileReader *reader, char **error_out);

/**
 * Select features within a bounding box. Consumes the reader.
 * Returns null on error.
 */
struct FcbFileIterator *fcb_reader_select_bbox(struct FcbFileReader *reader,
                                               double min_x,
                                               double min_y,
                                               double max_x,
                                               double max_y,
                                               char **error_out);

/**
 * Advance to the next feature. Returns 1 if a feature is available, 0 if done, -1 on error.
 */
int32_t fcb_iterator_next(struct FcbFileIterator *iter, char **error_out);

/**
 * Get the current feature as a JSON string.
 * Caller must free the returned string with `fcb_free_string`.
 */
char *fcb_iterator_current_json(const struct FcbFileIterator *iter, char **error_out);

/**
 * Get the current feature ID.
 * Caller must free the returned string with `fcb_free_string`.
 */
char *fcb_iterator_current_id(const struct FcbFileIterator *iter, char **error_out);

/**
 * Get the total features count from the iterator.
 */
uint64_t fcb_iterator_features_count(const struct FcbFileIterator *iter);

/**
 * Free a reader. Must be called when done with the reader.
 */
void fcb_reader_free(struct FcbFileReader *reader);

/**
 * Free an iterator. Must be called when done with the iterator.
 */
void fcb_iterator_free(struct FcbFileIterator *iter);

/**
 * Free a C string returned by any fcb_ function.
 */
void fcb_free_string(char *s);

#endif  /* FCB_CORE_H */
