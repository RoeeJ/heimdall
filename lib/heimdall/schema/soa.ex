defmodule Heimdall.Schema.SOA do
  use Ecto.Schema
  import Ecto.Changeset

  schema "soa_records" do
    field :mname, :string
    field :rname, :string
    belongs_to :zone, Heimdall.DNS.Zone

    timestamps()
  end

  def changeset(soa, attrs) do
    soa
    |> cast(attrs, [:mname, :rname, :zone_id])
    |> validate_required([:mname, :rname, :zone_id])
  end
end
