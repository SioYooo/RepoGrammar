from sqlalchemy.orm import Session


class UserRepository:
    def get_user(self, session: Session):
        return session.get(User, 1)

    def get_account(self, session: Session):
        return session.get(Account, 2)

    def get_team(self, session: Session):
        return session.get(Team, 3)


class User:
    pass


class Account:
    pass


class Team:
    pass
