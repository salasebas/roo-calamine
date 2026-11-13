# frozen_string_literal: true

# Loaded with -r before roo's own test suite. In plugin mode it loads
# roo-oxide; in both modes it counts how often roo's Ruby cell parser and
# the native reader run, so the report shows which reader served the suite.
require "json"
require "roo"
require "roo/excelx"

# roo's specs sometimes retain only a sheet or extractor. roo 4.0.0's
# workbook finalizer removes their files, so keep the owners alive in both
# runs until this suite's process exits. Explicit #close still takes effect.
workbooks = []
Roo::Excelx.singleton_class.prepend(Module.new do
  define_method(:new) do |*args, **options, &block|
    super(*args, **options, &block).tap { |workbook| workbooks << workbook }
  end
end)

counts = Hash.new(0)

if ENV.fetch("PARITY_MODE") == "plugin"
  require "roo/oxide"
  abort "roo-oxide did not enable itself" unless Roo::Oxide.enabled?

  Roo::Oxide::Native::Reader.singleton_class.prepend(Module.new do
    define_method(:new) do |*args|
      counts[:native_readers] += 1
      super(*args)
    end
  end)

  Roo::Oxide::Native::Store.singleton_class.prepend(Module.new do
    define_method(:read) do |*args|
      counts[:native_readers] += 1
      super(*args)
    end
  end)
end

Roo::Excelx::SheetDoc.prepend(Module.new do
  define_method(:cell_from_xml) do |*args|
    counts[:roo_cells] += 1
    super(*args)
  end
end)

out = ENV.fetch("PARITY_PROBE_OUT")
at_exit { File.write(out, JSON.generate(counts)) }
