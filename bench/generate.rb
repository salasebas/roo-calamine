# frozen_string_literal: true

# Writes the benchmark workbooks to bench/fixtures/. Run through bench/run.rb.
#
#   bench/generate.rb [FIXTURE...]   # default: every fixture
require "caxlsx"
require "csv"
require "date"
require "fileutils"
require "zip"
require "zlib"

DIR = File.join(__dir__, "fixtures")
WORDS = %w[alpha beta gamma delta epsilon zeta eta theta iota kappa].freeze

def png_chunk(type, data) = [data.bytesize].pack("N") + type + data + [Zlib.crc32(type + data)].pack("N")

PNG = "\x89PNG\r\n\x1A\n".b + png_chunk("IHDR", [1, 1, 8, 0, 0, 0, 0].pack("NNC5")) +
      png_chunk("IDAT", Zlib::Deflate.deflate("\x00\x80".b)) + png_chunk("IEND", "")

def mixed_values(index)
  [index, index * 1.5, "id-#{index}", WORDS[index % 10], Date.new(2020, 1, 1) + (index % 1000),
   index.even?, index % 7, (index % 100) / 4.0, "name #{index % 5000}", index * 3,
   -index, index / 3.0, WORDS[(index * 3) % 10], "x", 1.0, 42, Date.new(2021, 6, 1),
   index.even? ? "yes" : "no", index * 0.01, "tail"]
end

def mixed_row(index, date_style)
  styles = Array.new(20)
  styles[4] = styles[16] = date_style
  [mixed_values(index), styles]
end

def header(columns) = (1..columns).map { |i| "col#{i}" }

def add_mixed_sheet(workbook, name, rows)
  workbook.add_worksheet(name:) do |sheet|
    date_style = sheet.styles.add_style(format_code: "yyyy-mm-dd")
    sheet.add_row(header(20))
    rows.times do |index|
      values, styles = mixed_row(index, date_style)
      sheet.add_row(values, style: styles)
    end
  end
end

def write_mixed(path, sheets)
  package = Axlsx::Package.new
  sheets.each { |name, rows| add_mixed_sheet(package.workbook, name, rows) }
  package.serialize(path)
end

# Each cell holds a word and `suffix`: by default 500 strings in all.
def write_shared_strings(path, rows, suffix: ->(index, col) { (index * col) % 50 })
  package = Axlsx::Package.new
  package.use_shared_strings = true
  package.workbook.add_worksheet(name: "Data") do |sheet|
    sheet.add_row(header(10))
    rows.times do |index|
      sheet.add_row((0...10).map { |col| "#{WORDS[(index + col) % 10]} #{suffix.call(index, col)}" })
    end
  end
  package.serialize(path)
end

def write_wide(path, rows, columns)
  package = Axlsx::Package.new
  package.workbook.add_worksheet(name: "Data") do |sheet|
    sheet.add_row(header(columns))
    rows.times do |index|
      sheet.add_row((0...columns).map { |col| col.even? ? index * col : "#{WORDS[col % 10]} #{index}" })
    end
  end
  package.serialize(path)
end

def write_links_merges(path, rows)
  package = Axlsx::Package.new
  package.workbook.add_worksheet(name: "Data") do |sheet|
    sheet.add_row(header(5))
    rows.times do |index|
      line = index + 2
      sheet.add_row([index, "link #{index}", WORDS[index % 10], index * 2, nil])
      sheet.add_hyperlink(location: "https://example.com/#{index}", ref: "B#{line}")
      sheet.merge_cells("C#{line}:E#{line}") if (index % 10).zero?
    end
  end
  package.serialize(path)
end

def write_comments_formulas(path, rows)
  package = Axlsx::Package.new
  package.workbook.add_worksheet(name: "Data", escape_formulas: false) do |sheet|
    sheet.add_row(header(5))
    rows.times do |index|
      line = index + 2
      sheet.add_row([index, index * 1.5, WORDS[index % 10], "=A#{line}*2", "=C#{line}&\"!\""],
                    formula_values: [nil, nil, nil, index * 2, "#{WORDS[index % 10]}!"])
      sheet.add_comment(ref: "C#{line}", author: "bench", text: "note #{index}", visible: false) if (index % 10).zero?
    end
    image = File.join(DIR, "pixel.png")
    File.binwrite(image, PNG)
    sheet.add_image(image_src: image, start_at: "G2", width: 10, height: 10)
  end
  100.times { |i| package.workbook.add_defined_name("'Data'!$A$#{(i * 50) + 2}", name: "label_#{i}") }
  package.serialize(path)
end

def write_dates(path, rows)
  package = Axlsx::Package.new
  package.workbook.add_worksheet(name: "Data") do |sheet|
    formats = ["yyyy-mm-dd", "d-mmm-yy", "yyyy-mm-dd hh:mm:ss", "m/d/yy h:mm", "hh:mm:ss", "h:mm AM/PM", "[h]:mm:ss"]
    styles = formats.map { |format_code| sheet.styles.add_style(format_code:) }
    sheet.add_row(header(formats.size))
    start = Time.utc(2020, 1, 1)
    rows.times do |index|
      moment = start + (index * 3_607)
      fraction = (index % 86_400) / 86_400.0
      sheet.add_row([moment.to_date, moment.to_date, moment, moment, fraction, fraction, fraction * 3], style: styles)
    end
  end
  package.serialize(path)
end

def write_rich_text(path, rows)
  package = Axlsx::Package.new
  package.use_shared_strings = true
  package.workbook.add_worksheet(name: "Data") do |sheet|
    sheet.add_row(header(5))
    rows.times do |index|
      sheet.add_row((0...5).map do |col|
        Axlsx::RichText.new.tap do |text|
          text.add_run("#{WORDS[(index + col) % 10]} ", b: true)
          text.add_run("#{index}-#{col}", i: col.odd?)
        end
      end)
    end
  end
  package.serialize(path)
end

def rewrite_worksheets(path, source)
  Zip::OutputStream.open(path) do |out|
    Zip::File.open(source) do |zip|
      zip.each do |entry|
        data = entry.get_input_stream.read
        data = yield(data) if entry.name.start_with?("xl/worksheets/")
        out.put_next_entry(entry.name)
        out.write(data)
      end
    end
  end
end

DECLARATION = '<?xml version="1.0" encoding="UTF-8"?>'

def write_csv(path, rows)
  CSV.open(path, "w") do |csv|
    csv << header(20)
    rows.times { |index| csv << mixed_values(index) }
  end
end

def fixture(name) = File.join(DIR, name)

def mixed_10k
  fixture("mixed-10k.xlsx").tap { |path| write_mixed(path, "Data" => 10_000) unless File.exist?(path) }
end

FIXTURES = {
  "mixed-100k.xlsx" => -> { write_mixed(fixture("mixed-100k.xlsx"), "Data" => 100_000) },
  "shared-strings-100k.xlsx" => -> { write_shared_strings(fixture("shared-strings-100k.xlsx"), 100_000) },
  "unique-strings-100k.xlsx" => lambda {
    write_shared_strings(fixture("unique-strings-100k.xlsx"), 100_000, suffix: ->(index, col) { "#{index}-#{col}" })
  },
  "small.xlsx" => -> { write_mixed(fixture("small.xlsx"), "Data" => 100) },
  "mixed-10k.xlsx" => -> { mixed_10k },
  "large-1m.xlsx" => -> { write_mixed(fixture("large-1m.xlsx"), "Data" => 1_000_000) },
  "wide.xlsx" => -> { write_wide(fixture("wide.xlsx"), 100, 500) },
  "multi-sheet.xlsx" => lambda {
    write_mixed(fixture("multi-sheet.xlsx"), "Big" => 20_000, "Medium" => 5_000, "Small" => 1_000, "Tiny" => 100)
  },
  "links-merges-10k.xlsx" => -> { write_links_merges(fixture("links-merges-10k.xlsx"), 10_000) },
  "comments-formulas-10k.xlsx" => -> { write_comments_formulas(fixture("comments-formulas-10k.xlsx"), 10_000) },
  "dates-10k.xlsx" => -> { write_dates(fixture("dates-10k.xlsx"), 10_000) },
  "rich-text-10k.xlsx" => -> { write_rich_text(fixture("rich-text-10k.xlsx"), 10_000) },
  "fallback-10k.xlsx" => lambda {
    rewrite_worksheets(fixture("fallback-10k.xlsx"), mixed_10k) { _1.sub('encoding="UTF-8"', 'encoding="ISO-8859-1"') }
  },
  "fallback-dtd-10k.xlsx" => lambda {
    rewrite_worksheets(fixture("fallback-dtd-10k.xlsx"), mixed_10k) do |xml|
      xml.sub(DECLARATION, %(#{DECLARATION}<!DOCTYPE worksheet [<!ENTITY bench "bench">]>))
    end
  },
  "number-text-10k.xlsx" => lambda {
    rewrite_worksheets(fixture("number-text-10k.xlsx"), mixed_10k) do |xml|
      xml.gsub(%r{t="n"><v>(\d+)</v>}, 't="n"><v>\\1.00E0</v>')
    end
  },
  "csv-10k.csv" => -> { write_csv(fixture("csv-10k.csv"), 10_000) }
}.freeze

FileUtils.mkdir_p(DIR)
(ARGV.empty? ? FIXTURES.keys : ARGV).each do |name|
  next if File.exist?(fixture(name))

  warn "writing bench/fixtures/#{name}"
  FIXTURES.fetch(name).call
end
