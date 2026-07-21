from pydantic import BaseModel


class Item31(BaseModel):
    id: int
    label: str
    quantity: int
