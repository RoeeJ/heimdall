defmodule Heimdall.DNS.Model.Nameserver do
  def parse(nameservers, data, 0), do: [nameservers, data]

  def parse(nameservers, data, count) do
    parse(nameservers, data, count - 1)
  end

  def encode(__MODULE__ = answer) do
    <<>>
  end
end
