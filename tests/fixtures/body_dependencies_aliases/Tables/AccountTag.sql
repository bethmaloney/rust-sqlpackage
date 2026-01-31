-- Junction table linking accounts to tags
CREATE TABLE [dbo].[AccountTag]
(
    [AccountId] INT NOT NULL,
    [TagId] INT NOT NULL,
    CONSTRAINT [PK_AccountTag] PRIMARY KEY ([AccountId], [TagId]),
    CONSTRAINT [FK_AccountTag_Account] FOREIGN KEY ([AccountId]) REFERENCES [dbo].[Account] ([Id]),
    CONSTRAINT [FK_AccountTag_Tag] FOREIGN KEY ([TagId]) REFERENCES [dbo].[Tag] ([Id])
);
GO
