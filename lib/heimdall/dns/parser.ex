defmodule Heimdall.DNS.Parser do
  alias Heimdall.DNS.Model

  @spec parse(data :: binary()) :: Model.Packet.t()
  def parse(data) do
    <<id::16, qr::1, opcode::4, aa::1, tc::1, rd::1, ra::1, z::3, rcode::4, data::bitstring>> =
      data

    <<qdcount::16, ancount::16, nscount::16, arcount::16, data::bitstring>> = data

    [questions, data] = Model.Question.parse([], data, qdcount)
    [answers, data] = Model.Answer.parse([], data, ancount)
    [nameservers, data] = Model.Nameserver.parse([], data, nscount)
    [additional, data] = Model.Additional.parse([], data, arcount)

    %Model.Packet{
      id: id,
      qr: Model.qr(qr),
      opcode: Model.opcode(opcode),
      aa: aa,
      tc: tc,
      rd: rd,
      ra: ra,
      z: z,
      rcode: rcode,
      qdcount: qdcount,
      ancount: ancount,
      nscount: nscount,
      arcount: arcount,
      questions: questions,
      answers: answers,
      nameservers: nameservers,
      additional: additional
    }
  end
end
