defmodule Heimdall.DNS.Encoder do
  alias Heimdall.DNS.Model

  def packet(%Model.Packet{} = packet) do
    header = encode_header(packet)
    questions = Enum.map(packet.questions, &encode_question/1)
    answers = Enum.map(packet.answers, &Model.ResourceRecord.encode/1)
    authority = Enum.map(packet.nameservers, &Model.ResourceRecord.encode/1)
    additional = Enum.map(packet.additional, &Model.ResourceRecord.encode/1)

    {:ok,
     header <>
       Enum.join(questions) <> Enum.join(answers) <> Enum.join(authority) <> Enum.join(additional)}
  end

  defp encode_header(packet) do
    <<
      packet.id::16,
      qr(packet.qr)::1,
      opcode(packet.opcode)::4,
      aa(packet.authoritative)::1,
      tc(packet.truncated)::1,
      rd(packet.recurs_desired)::1,
      ra(packet.recurs_available)::1,
      packet.z::3,
      rcode(packet.resp_code)::4,
      packet.qdcount::16,
      packet.ancount::16,
      packet.nscount::16,
      packet.arcount::16
    >>
  end

  defp encode_question(%Model.Question{} = q) do
    labels(q.qname) <> <<qtype(q.qtype)::16, qclass(q.qclass)::16>>
  end

  def labels(""), do: <<0::8>>
  def labels("."), do: <<0::8>>

  def labels(name) when is_binary(name) do
    name
    |> String.split(".")
    |> Enum.map(fn part -> <<byte_size(part)::8, part::binary>> end)
    |> Enum.join()
    |> Kernel.<>(<<0>>)
  end

  def labels(name) when is_list(name) do
    name
    |> Enum.map(fn part -> <<byte_size(part)::8, part::binary>> end)
    |> Enum.join()
    |> Kernel.<>(<<0>>)
  end

  def qr(:query), do: 0
  def qr(:response), do: 1

  def opcode(:query), do: 0
  def opcode(:iquery), do: 1
  def opcode(:status), do: 2

  def aa(true), do: 1
  def aa(false), do: 0

  def tc(true), do: 1
  def tc(false), do: 0

  def rd(true), do: 1
  def rd(false), do: 0

  def ra(true), do: 1
  def ra(false), do: 0

  def rcode(:no_err), do: 0
  def rcode(:server_err), do: 1
  def rcode(:format_err), do: 2
  def rcode(:name_err), do: 3
  def rcode(:not_implemented), do: 4
  def rcode(:refused), do: 5

  def qtype(:a), do: 1
  def qtype(:ns), do: 2
  def qtype(:cname), do: 5
  def qtype(:soa), do: 6
  def qtype(:ptr), do: 12
  def qtype(:mx), do: 15
  def qtype(:txt), do: 16
  def qtype(:aaaa), do: 28
  def qtype(:srv), do: 33
  def qtype(:opt), do: 41

  def qclass(:in), do: 1
  def qclass(:cs), do: 2
  def qclass(:ch), do: 3
  def qclass(:hs), do: 4
end
