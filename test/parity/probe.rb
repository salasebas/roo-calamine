# frozen_string_literal: true

# Loaded with -r before roo's own test suite. In plugin mode it loads
# roo-calamine; in both modes it counts how often roo's Ruby cell parser and
# the native reader run, so the report shows which reader served the suite.
require "json"
require "roo"
require "roo/excelx"

counts = Hash.new(0)

if ENV.fetch("PARITY_MODE") == "plugin"
  require "roo/calamine"
  abort "roo-calamine did not enable itself" unless Roo::Calamine.enabled?

  Roo::Calamine::Native::Reader.singleton_class.prepend(Module.new do
    define_method(:new) do |*args|
      counts[:native_readers] += 1
      super(*args)
    end
  end)

  Roo::Calamine::Native::Store.singleton_class.prepend(Module.new do
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
