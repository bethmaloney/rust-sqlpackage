-- Table with self-referencing foreign key (employee hierarchy)
CREATE TABLE [dbo].[Employees] (
    [Id] INT NOT NULL,
    [Name] NVARCHAR(200) NOT NULL,
    [Email] NVARCHAR(255) NOT NULL,
    [ManagerId] INT NULL,  -- References same table

    CONSTRAINT [PK_Employees] PRIMARY KEY ([Id]),

    -- Self-referencing FK: ManagerId points to another Employee
    CONSTRAINT [FK_Employees_Manager] FOREIGN KEY ([ManagerId]) REFERENCES [dbo].[Employees]([Id])
);
GO
