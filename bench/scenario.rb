# frozen_string_literal: true

# One benchmark run in a fresh process:
#
#   bench/scenario.rb LIBRARY SCENARIO FILE [digest|profile]
#
# Prints JSON with the wall time, the peak RSS, and the objects allocated by
# this process. With `digest`, it also hashes every value read, so the values
# roo and roo-oxide return can be compared; that run is not timed in the
# table. With `profile`, it records the run with vernier into
# bench/profiles/LIBRARY-SCENARIO-FILE.json.
require "digest"
require "fiddle"
require "json"
require "stringio"

library, scenario, path, mode = ARGV
DIGESTING = mode == "digest"
case library
when "roo" then require "roo"
when "roo-oxide" then require "roo/oxide"
when "calx" then require "calx"
when "xsv" then require "xsv"
end
# String#inspect hides a Roo::Link's target.
if DIGESTING && library.start_with?("roo")
  Roo::Link.class_eval { def inspect = "#<Roo::Link #{href.inspect} #{super}>" }
end

def peak_rss_bytes
  rusage = Fiddle::Pointer.malloc(256, Fiddle::RUBY_FREE)
  getrusage = Fiddle::Function.new(Fiddle::Handle::DEFAULT["getrusage"], [Fiddle::TYPE_INT, Fiddle::TYPE_VOIDP],
                                   Fiddle::TYPE_INT)
  getrusage.call(0, rusage)
  maxrss = rusage[32, 8].unpack1("q")
  RUBY_PLATFORM.include?("darwin") ? maxrss : maxrss * 1024
end

RANDOM_CELLS = 10_000
SET_CELLS = 1_000

def value(object)
  DIGESTING ? object.inspect : object
end

def open_workbook(path, **) = Roo::Spreadsheet.open(path, **)

def random_reads(workbook, digest)
  random = Random.new(1)
  rows = workbook.last_row
  columns = workbook.last_column
  RANDOM_CELLS.times { digest << value(yield(random.rand(1..rows), random.rand(1..columns))) }
end

# roo's `set` warns on every call that `Cell.new` is deprecated.
def random_sets(workbook)
  random = Random.new(2)
  rows = workbook.last_row
  columns = workbook.last_column
  verbose = $VERBOSE
  $VERBOSE = nil
  SET_CELLS.times { |i| workbook.set(random.rand(1..rows), random.rand(1..columns), "set #{i}") }
ensure
  $VERBOSE = verbose
end

def each_value(workbook, digest, **options)
  workbook.each(**options) { |row| digest << value(row) }
end

def stream_values(workbook, digest, **options)
  workbook.each_row_streaming(**options) { |row| row.each { digest << value(_1&.value) } }
end

def font_value(font) = font && [font.bold?, font.italic?, font.underline?]

# Addresses in `inspect` differ from run to run.
def without_addresses(text) = text.gsub(/0x\h+/, "")

CELL_READS = {
  "celltype" => :celltype, "formula" => :formula, "is-formula" => :formula?, "excelx_type" => :excelx_type,
  "excelx_value" => :excelx_value, "formatted_value" => :formatted_value, "excelx_format" => :excelx_format,
  "is-empty" => :empty?, "hyperlink" => :hyperlink, "is-hyperlink" => :hyperlink?, "comment" => :comment,
  "is-comment" => :comment?
}.freeze

ROO_SCENARIOS = {
  "stream" => ->(path, digest) { stream_values(open_workbook(path), digest) },
  "parse" => ->(path, digest) { open_workbook(path).parse(headers: true).each { |row| digest << value(row.values) } },
  "each" => ->(path, digest) { each_value(open_workbook(path), digest) },
  "cell" => ->(path, digest) { open_workbook(path).then { |w| random_reads(w, digest) { |r, c| w.cell(r, c) } } },
  "csv" => ->(path, digest) { digest << open_workbook(path).to_csv },
  "yaml" => ->(path, digest) { digest << open_workbook(path).to_yaml },
  "xml" => ->(path, digest) { digest << open_workbook(path).to_xml },
  "matrix" => ->(path, digest) { digest << value(open_workbook(path).to_matrix) },

  "open" => ->(path, digest) { digest << value(Roo::Excelx.new(path).sheets) },
  "open-spreadsheet" => ->(path, digest) { digest << value(open_workbook(path).sheets) },
  "open-stringio" => ->(path, digest) { digest << value(Roo::Excelx.new(StringIO.new(File.binread(path))).sheets) },
  "open-file" => ->(path, digest) { File.open(path, "rb") { digest << value(Roo::Excelx.new(_1).sheets) } },

  "row" => lambda { |path, digest|
    workbook = open_workbook(path)
    workbook.first_row.upto(workbook.last_row) { digest << value(workbook.row(_1)) }
  },
  "column" => lambda { |path, digest|
    workbook = open_workbook(path)
    workbook.first_column.upto(workbook.last_column) { digest << value(workbook.column(_1)) }
  },
  "bounds" => lambda { |path, digest|
    workbook = open_workbook(path)
    digest << value(%i[first_row last_row first_column last_column first_column_as_letter
                       last_column_as_letter].map { workbook.public_send(_1) })
  },
  "find" => ->(path, digest) { digest << value(open_workbook(path).find(:all, conditions: { "col4" => "alpha" })) },
  "row_with" => ->(path, digest) { digest << value(open_workbook(path).row_with(%w[col1 col4 col20])) },

  "font" => lambda { |path, digest|
    workbook = open_workbook(path)
    random_reads(workbook, digest) { |row, col| font_value(workbook.font(row, col)) }
  },
  "formulas" => ->(path, digest) { digest << value(open_workbook(path).formulas) },

  "each-headers" => ->(path, digest) { each_value(open_workbook(path), digest, headers: true) },
  "each-header-search" => ->(path, digest) { each_value(open_workbook(path), digest, header_search: %w[col1 col4]) },
  "each-query" => ->(path, digest) { each_value(open_workbook(path), digest, id: "col1", word: "col4", day: "col5") },
  "each-clean" => ->(path, digest) { each_value(open_workbook(path), digest, clean: true, headers: true) },
  "each_with_pagename" => lambda { |path, digest|
    open_workbook(path).each_with_pagename { |name, sheet| sheet.each { digest << value([name, _1]) } }
  },
  "parse-block" => lambda { |path, digest|
    digest << value(open_workbook(path).parse(headers: true, &:values))
  },
  "cell-then-clean" => lambda { |path, digest|
    workbook = open_workbook(path)
    digest << value(workbook.cell(2, 1))
    each_value(workbook, digest, clean: true, headers: true)
  },
  "stream-offset" => ->(path, digest) { stream_values(open_workbook(path), digest, offset: 5_000) },
  "stream-max-rows" => ->(path, digest) { stream_values(open_workbook(path), digest, max_rows: 1_000) },
  "stream-pad" => ->(path, digest) { stream_values(open_workbook(path), digest, pad_cells: true) },
  "stream-stringio" => ->(path, digest) { stream_values(Roo::Excelx.new(StringIO.new(File.binread(path))), digest) },

  "cells" => lambda { |path, digest|
    workbook = open_workbook(path)
    workbook.sheet_for(workbook.default_sheet).cells.each { |key, cell| digest << value([key, cell.class, cell.value]) }
  },
  "to_s" => lambda { |path, digest|
    text = open_workbook(path).to_s
    digest << (DIGESTING ? without_addresses(text) : text)
  },

  "set-each" => lambda { |path, digest|
    workbook = open_workbook(path)
    random_sets(workbook)
    each_value(workbook, digest)
  },
  "reload" => lambda { |path, digest|
    workbook = open_workbook(path)
    random_sets(workbook)
    workbook.reload
    each_value(workbook, digest)
  },

  "sheets" => ->(path, digest) { digest << value(open_workbook(path).sheets) },
  "sheet" => lambda { |path, digest|
    workbook = open_workbook(path)
    workbook.sheets.each_index { digest << value(workbook.sheet(_1).cell(2, 1)) }
  },
  "dimensions" => lambda { |path, digest|
    workbook = open_workbook(path)
    workbook.sheets.each { digest << value(workbook.sheet_for(_1).dimensions) }
  },
  "hyperlinks" => lambda { |path, digest|
    workbook = open_workbook(path)
    digest << value(workbook.sheet_for(workbook.default_sheet).hyperlinks)
  },
  "comments" => ->(path, digest) { digest << value(open_workbook(path).comments) },
  "labels" => ->(path, digest) { digest << value(open_workbook(path).labels) },
  "images" => ->(path, digest) { digest << value(open_workbook(path).images.map { File.basename(_1) }) },

  "expand_merged_ranges" => ->(path, digest) { each_value(open_workbook(path, expand_merged_ranges: true), digest) },
  "disable_html_wrapper" => ->(path, digest) { each_value(open_workbook(path, disable_html_wrapper: true), digest) },
  "cell_max" => ->(path, digest) { each_value(open_workbook(path, cell_max: 10_000_000), digest) },
  "no_hyperlinks" => ->(path, digest) { each_value(open_workbook(path, no_hyperlinks: true), digest) }
}.merge(CELL_READS.transform_values do |method|
  ->(path, digest) { open_workbook(path).then { |w| random_reads(w, digest) { |r, c| w.public_send(method, r, c) } } }
end).freeze

def values_scenario(library, path, digest)
  if library == "roo-oxide"
    Roo::Oxide.each_row_values(path) { |row| row.each { |cell_value| digest << value(cell_value) } }
  else
    stream_values(open_workbook(path), digest, pad_cells: true)
  end
end

def reference_random_cells(rows, digest)
  random = Random.new(1)
  RANDOM_CELLS.times do
    row = rows[random.rand(rows.size)]
    digest << value(row[random.rand(row.size)])
  end
end

def reference_parse(rows, digest)
  header, *rest = rows
  rest.each { |row| digest << value(header.zip(row).to_h.values) }
end

# calx and xsv each yield one Array of values per row.
def reference_scenarios(each_row)
  values = ->(path, digest) { each_row.call(path) { |row| row.each { digest << value(_1) } } }
  {
    "stream" => values,
    "values" => values,
    "each" => ->(path, digest) { each_row.call(path) { |row| digest << value(row) } },
    "parse" => ->(path, digest) { reference_parse(each_row.call(path).to_a, digest) },
    "cell" => ->(path, digest) { reference_random_cells(each_row.call(path).to_a, digest) },
    "csv" => ->(path, digest) { each_row.call(path) { |row| digest << row.join(",") } }
  }.freeze
end

CALX_SCENARIOS = reference_scenarios(->(path, &block) { block ? Calx.each_row(path, &block) : Calx.read(path) })
XSV_SCENARIOS = reference_scenarios(->(path, &block) { Xsv.open(path).sheets.first.each_row(&block) })

# Timed runs only touch each value; `<<` on this sink does nothing.
class Sink
  def <<(_value) = self
  def hexdigest = nil
end

run = lambda do |digest|
  case library
  when "calx" then CALX_SCENARIOS.fetch(scenario).call(path, digest)
  when "xsv" then XSV_SCENARIOS.fetch(scenario).call(path, digest)
  else scenario == "values" ? values_scenario(library, path, digest) : ROO_SCENARIOS.fetch(scenario).call(path, digest)
  end
end

GC.start
digest = DIGESTING ? Digest::SHA256.new : Sink.new
allocated = GC.stat(:total_allocated_objects)
started = Process.clock_gettime(Process::CLOCK_MONOTONIC)
if mode == "profile"
  require "fileutils"
  require "vernier"
  out = File.join(__dir__, "profiles", "#{library}-#{scenario}-#{File.basename(path)}.json")
  FileUtils.mkdir_p(File.dirname(out))
  Vernier.profile(out:) { run.call(digest) }
else
  run.call(digest)
end
seconds = Process.clock_gettime(Process::CLOCK_MONOTONIC) - started
puts JSON.generate(seconds:, peak_rss: peak_rss_bytes, allocations: GC.stat(:total_allocated_objects) - allocated,
                   digest: digest.hexdigest)
