# frozen_string_literal: true

source "https://rubygems.org"

gemspec

gem "minitest", "~> 6.1"
gem "rake", "~> 13.3"
gem "rake-compiler", "~> 1.3"

group :lint do
  gem "rubocop", require: false
  gem "rubocop-minitest", require: false
  gem "rubocop-performance", require: false
  gem "rubocop-rake", require: false
end

group :security do
  gem "bundler-audit", require: false
end

# What roo 4.0.0's own test suite needs, for the parity run (rake parity).
group :parity do
  gem "activesupport"
  gem "matrix"
  gem "minitest-reporters"
  gem "rack"
  gem "rackup"
  gem "rspec", "~> 3.13"
  gem "shoulda"
  gem "simplecov", require: false
  gem "webrick"
end
