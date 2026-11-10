# frozen_string_literal: true

# Writes the benchmark workbooks to bench/fixtures/. Run through bench/run.rb.
require "caxlsx"
require "date"
require "fileutils"

DIR = File.join(__dir__, "fixtures")
WORDS = %w[alpha beta gamma delta epsilon zeta eta theta iota kappa].freeze

def mixed_row(index, date_style)
  values = [index, index * 1.5, "id-#{index}", WORDS[index % 10], Date.new(2020, 1, 1) + (index % 1000),
            index.even?, index % 7, (index % 100) / 4.0, "name #{index % 5000}", index * 3,
            -index, index / 3.0, WORDS[(index * 3) % 10], "x", 1.0, 42, Date.new(2021, 6, 1),
            index.even? ? "yes" : "no", index * 0.01, "tail"]
  styles = Array.new(20)
  styles[4] = styles[16] = date_style
  [values, styles]
end

def write_mixed(path, rows)
  package = Axlsx::Package.new
  package.workbook.add_worksheet(name: "Data") do |sheet|
    date_style = sheet.styles.add_style(format_code: "yyyy-mm-dd")
    sheet.add_row((1..20).map { |i| "col#{i}" })
    rows.times do |index|
      values, styles = mixed_row(index, date_style)
      sheet.add_row(values, style: styles)
    end
  end
  package.serialize(path)
end

# Each cell holds a word and `suffix`: by default 500 strings in all.
def write_shared_strings(path, rows, suffix: ->(index, col) { (index * col) % 50 })
  package = Axlsx::Package.new
  package.use_shared_strings = true
  package.workbook.add_worksheet(name: "Data") do |sheet|
    sheet.add_row((1..10).map { |i| "col#{i}" })
    rows.times do |index|
      sheet.add_row((0...10).map { |col| "#{WORDS[(index + col) % 10]} #{suffix.call(index, col)}" })
    end
  end
  package.serialize(path)
end

FileUtils.mkdir_p(DIR)
{
  "mixed-100k.xlsx" => -> { write_mixed(File.join(DIR, "mixed-100k.xlsx"), 100_000) },
  "shared-strings-100k.xlsx" => -> { write_shared_strings(File.join(DIR, "shared-strings-100k.xlsx"), 100_000) },
  "unique-strings-100k.xlsx" => lambda {
    write_shared_strings(File.join(DIR, "unique-strings-100k.xlsx"), 100_000,
                         suffix: ->(index, col) { "#{index}-#{col}" })
  },
  "small.xlsx" => -> { write_mixed(File.join(DIR, "small.xlsx"), 100) }
}.each do |name, write|
  next if File.exist?(File.join(DIR, name))

  warn "writing bench/fixtures/#{name}"
  write.call
end
