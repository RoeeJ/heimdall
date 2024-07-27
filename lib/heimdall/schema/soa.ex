defmodule Heimdall.Schema.SOA do
  @moduledoc """
  SOA record schema
  """
  use Ecto.Schema
  import Ecto.Changeset

  @type t() :: %__MODULE__{
          id: non_neg_integer(),
          inserted_at: NaiveDateTime.t(),
          updated_at: NaiveDateTime.t(),
          mname: String.t(),
          rname: String.t(),
          zone: Heimdall.Schema.Zone.t()
        }

  schema "soa_records" do
    field :mname, :string
    field :rname, :string
    belongs_to :zone, Heimdall.Schema.Zone

    timestamps()
  end

  def changeset(soa, attrs) do
    soa
    |> cast(attrs, [:mname, :rname, :zone_id])
    |> validate_required([:mname, :rname, :zone_id])
  end
end
