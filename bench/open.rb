# frozen_string_literal: true

require "json"
require "roo/calamine"

module OpeningProfile
  @steps = Hash.new { |steps, name| steps[name] = 0.0 }
  @stack = []

  class << self
    attr_reader :steps, :stack

    def clock = Process.clock_gettime(Process::CLOCK_MONOTONIC)

    def watch(owner, method, name)
      owner.prepend(Module.new do
        define_method(method) do |*args, **options, &block|
          frame = [OpeningProfile.clock, 0.0]
          OpeningProfile.stack << frame
          super(*args, **options, &block)
        ensure
          elapsed = OpeningProfile.clock - frame.first
          OpeningProfile.stack.pop
          OpeningProfile.stack.last[1] += elapsed unless OpeningProfile.stack.empty?
          OpeningProfile.steps[name] += elapsed - frame.last
        end
      end)
    end

    def elapsed
      started = clock
      yield
      (clock - started) * 1000
    end
  end
end

OpeningProfile.watch(Roo::Excelx, :initialize, "other open")
OpeningProfile.watch(Roo::Excelx.singleton_class, :make_tempdir, "tmpdir")
OpeningProfile.watch(Roo::Calamine::DeferredSheets, :write_calamine_copy, "snapshot")
OpeningProfile.watch(Zip::CentralDirectory, :read_from_stream, "central directory")
OpeningProfile.watch(Roo::Calamine::DeferredSheets, :native_entries, "native entries")
OpeningProfile.watch(Roo::Calamine::DeferredSheets, :extract_worksheet_ids, "sheet IDs")
OpeningProfile.watch(Roo::Calamine::DeferredSheets, :extract_worksheet_rels, "relationships")
OpeningProfile.watch(Roo::Calamine::DeferredSheets, :process_zipfile_entries, "other extraction")
OpeningProfile.watch(Roo::Excelx::Workbook, :doc, "workbook XML")
OpeningProfile.watch(Roo::Excelx::Styles, :doc, "styles XML")
OpeningProfile.watch(Roo::Excelx::Sheet, :initialize, "Sheet objects")
OpeningProfile.watch(Roo::Calamine::Native.singleton_class, :metadata_bytes, "metadata inflation")
OpeningProfile.watch(Roo::Calamine::Native::Reader.singleton_class, :new, "reader startup")

path = ARGV.fetch(0) { File.join(__dir__, "fixtures", "small.xlsx") }
GC.start
workbook = nil
open_ms = OpeningProfile.elapsed { workbook = Roo::Excelx.new(path) }
open_steps = OpeningProfile.steps.transform_values { _1 * 1000 }
OpeningProfile.steps.clear
stream_ms = OpeningProfile.elapsed { workbook.each_row_streaming { |row| row.each(&:value) } }
stream_steps = OpeningProfile.steps.transform_values { _1 * 1000 }
workbook.close

workbook = Roo::Excelx.new(path)
sheet = workbook.sheet_for(nil)
store = sheet.lazy_cells
random = Random.new(1)
coordinates = Array.new(10_000) { [random.rand(1..workbook.last_row), random.rand(1..workbook.last_column)] }
first_cells_ms = OpeningProfile.elapsed { coordinates.each { |row, column| workbook.cell(row, column) } }
cell_ms = {
  first_cells: first_cells_ms,
  cached_cells: OpeningProfile.elapsed { coordinates.each { |row, column| workbook.cell(row, column) } },
  cached_sheet: OpeningProfile.elapsed { coordinates.each { |row, column| sheet.cell_at(row, column)&.value } },
  cached_store: OpeningProfile.elapsed { coordinates.each { |row, column| store.cell_at(row, column)&.value } }
}
workbook.close
puts JSON.pretty_generate(open_ms:, open_steps_ms: open_steps, stream_ms:, stream_steps_ms: stream_steps, cell_ms:)
