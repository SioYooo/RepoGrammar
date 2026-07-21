from pydantic import BaseModel


class Item37(BaseModel):
    id: int
    label: str
    quantity: int
