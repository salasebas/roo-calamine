# ZIP control

Ruby 3.4.12, rubyzip as locked by the experiment, zip 9.5.1.
The Rust probe source is in `zip-control.rs.txt`.

The two files contain one stored entry named `entry`, holding `hello`.
The damaged file replaces both local and central CRC fields with 123,
without changing any other byte. The local CRC starts at offset 14;
the central CRC starts 16 bytes after the `PK\x01\x02` signature.

```ruby
Zip::OutputStream.open("good.zip") do |zip|
  zip.put_next_entry("entry", nil, nil, Zip::Entry::STORED)
  zip.write("hello")
end
bytes = File.binread("good.zip")
bytes[14, 4] = [123].pack("V")
central = bytes.index("PK\x01\x02")
bytes[central + 16, 4] = [123].pack("V")
File.binwrite("crc.zip", bytes)
%w[good crc].each do |name|
  Zip::File.open("#{name}.zip") { |zip| p [name, zip.read("entry")] }
end
```

Observed rubyzip output:

```text
["good", "hello"]
["crc", "hello"]
```

Observed Rust output, valid file then damaged file:

```text
Ok(5)
"hello"
Err(Custom { kind: InvalidData, error: "Invalid checksum" })
"hello"
```

Returning the bytes alongside an error does not preserve roo's successful
read. This blocks claiming a direct archive replacement without further
compatibility work. No archive deletion is attributed to this control.
