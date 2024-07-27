defmodule Heimdall.DNS.Encoder do
  alias Heimdall.DNS.Model

  def packet(%Model.Packet{} = packet) do
    header = encode_header(packet)
    questions = Enum.map(packet.questions, &encode_question/1)
    answers = Enum.map(packet.answers, &encode_resource_record/1)
    authority = Enum.map(packet.nameservers, &encode_resource_record/1)
    additional = Enum.map(packet.additional, &encode_resource_record/1)

    {:ok,
     header <>
       Enum.join(questions) <> Enum.join(answers) <> Enum.join(authority) <> Enum.join(additional)}
  end

  defp encode_resource_record(%Model.ResourceRecord{} = rr) do
    encoded_name = labels(rr.qname)
    encoded_type = <<qtype(rr.qtype)::16>>
    encoded_class = <<qclass(rr.qclass)::16>>
    encoded_ttl = <<rr.ttl::32>>
    {encoded_rdata, rdlength} = encode_rdata(rr.rdata, rr.qtype)
    encoded_rdlength = <<rdlength::16>>

    encoded_name <> encoded_type <> encoded_class <> encoded_ttl <> encoded_rdlength <> encoded_rdata
  end

  defp encode_rdata(rdata, :a) when is_tuple(rdata) do
    data = <<elem(rdata, 0)::8, elem(rdata, 1)::8, elem(rdata, 2)::8, elem(rdata, 3)::8>>
    {data, byte_size(data)}
  end

  defp encode_rdata(rdata, :aaaa) when is_tuple(rdata) do
    data = <<elem(rdata, 0)::16, elem(rdata, 1)::16, elem(rdata, 2)::16, elem(rdata, 3)::16,
      elem(rdata, 4)::16, elem(rdata, 5)::16, elem(rdata, 6)::16, elem(rdata, 7)::16>>
    {data, byte_size(data)}
  end

  defp encode_rdata(rdata, :cname) when is_binary(rdata) do
    data = labels(rdata)
    {data, byte_size(data)}
  end

  defp encode_rdata(rdata, :ns) when is_binary(rdata) do
    data = labels(rdata)
    {data, byte_size(data)}
  end

  defp encode_rdata(rdata, :mx) do
    data = <<rdata.preference::16>> <> labels(rdata.exchange)
    {data, byte_size(data)}
  end

  defp encode_rdata(rdata, :txt) when is_binary(rdata) do
    data = <<byte_size(rdata)::8, rdata::binary>>
    {data, byte_size(data)}
  end

  defp encode_rdata(%Model.EDNS{} = edns, :opt) do
    encoded_options = Enum.map(edns.options, &encode_edns_option/1) |> Enum.join()
    {encoded_options, byte_size(encoded_options)}
  end

  defp encode_edns_option({:cookie, cookie}) do
    encoded_cookie = Model.EDNS.Cookie.encode(cookie)
    <<10::16, byte_size(encoded_cookie)::16, encoded_cookie::binary>>
  end

  defp encode_edns_option({code, data}) do
    <<code::16, byte_size(data)::16, data::binary>>
  end

  defp encode_rdata(rdata, _type) when is_binary(rdata) do
    {rdata, byte_size(rdata)}
  end

  defp encode_header(%Model.Packet{header: header} = packet) do
    <<
      header.id::16,
      qr(header.qr)::1,
      opcode(header.opcode)::4,
      aa(header.authoritative)::1,
      tc(header.truncated)::1,
      rd(header.recurs_desired)::1,
      ra(header.recurs_available)::1,
      header.z::3,
      rcode(header.resp_code)::4,
      length(packet.questions)::16,
      length(packet.answers)::16,
      length(packet.nameservers)::16,
      length(packet.additional)::16
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

  def qr(false), do: 0
  def qr(true), do: 1

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

  defp rcode(:noerror), do: 0
  defp rcode(:formerr), do: 1
  defp rcode(:servfail), do: 2
  defp rcode(:nxdomain), do: 3
  defp rcode(:notimp), do: 4
  defp rcode(:refused), do: 5
  defp rcode(:yxdomain), do: 6
  defp rcode(:yxrrset), do: 7
  defp rcode(:nxrrset), do: 8
  defp rcode(:notauth), do: 9
  defp rcode(:notzone), do: 10

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
  def qtype(:svcb), do: 64
  def qtype(:https), do: 65
  def qtype(:any), do: 255
  def qtype(n) when is_integer(n), do: n

  def qclass(:in), do: 1
  def qclass(:cs), do: 2
  def qclass(:ch), do: 3
  def qclass(:hs), do: 4
  def qclass(n) when is_integer(n), do: n
end
