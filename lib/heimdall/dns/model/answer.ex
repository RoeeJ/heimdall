defmodule Heimdall.DNS.Model.Answer do
  def parse(answers, data, 0), do: [answers, data]

  def parse(answers, data, count) do
  end

  def encode(__MODULE__ = answer) do
    <<>>
  end
end
